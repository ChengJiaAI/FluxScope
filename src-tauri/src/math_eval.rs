use std::collections::HashMap;

/// Pure Rust recursive-descent AST expression compiler & vectorized evaluator for FOC derived channels.
/// Supports: +, -, *, /, %, ^ or ** (power), unary +/-, variables (Vd, Vq, CH0..CHn, pi, e),
/// and functions: sqrt, abs, sin, cos, tan, asin, acos, atan, atan2, hypot, exp, log, log10,
/// maximum/max, minimum/min, clip(x, lo, hi), deg2rad, rad2deg, lpf(x, alpha), movavg(x, win), rms_win(x, win), diff(x, dt).
#[derive(Debug, Clone)]
pub enum ExprNode {
    Const(f32),
    Var(String),
    UnaryNeg(Box<ExprNode>),
    Binary {
        op: BinOp,
        lhs: Box<ExprNode>,
        rhs: Box<ExprNode>,
    },
    Call {
        func: String,
        args: Vec<ExprNode>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f32),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            let v = s
                .parse::<f32>()
                .map_err(|_| format!("无效数值常量 '{}'", s))?;
            out.push(Token::Num(v));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || (c as u32) > 127 {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || (chars[i] as u32) > 127)
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push(Token::Ident(s));
            continue;
        }
        match c {
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push(Token::Caret);
                    i += 2;
                } else {
                    out.push(Token::Star);
                    i += 1;
                }
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '%' => {
                out.push(Token::Percent);
                i += 1;
            }
            '^' => {
                out.push(Token::Caret);
                i += 1;
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            ',' => {
                out.push(Token::Comma);
                i += 1;
            }
            _ => return Err(format!("表达式包含不支持字符 '{}'", c)),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn parse_expr(&mut self) -> Result<ExprNode, String> {
        let mut node = self.parse_term()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.next();
            let rhs = self.parse_term()?;
            node = ExprNode::Binary {
                op,
                lhs: Box::new(node),
                rhs: Box::new(rhs),
            };
        }
        Ok(node)
    }

    fn parse_term(&mut self) -> Result<ExprNode, String> {
        let mut node = self.parse_power()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Rem,
                _ => break,
            };
            self.next();
            let rhs = self.parse_power()?;
            node = ExprNode::Binary {
                op,
                lhs: Box::new(node),
                rhs: Box::new(rhs),
            };
        }
        Ok(node)
    }

    fn parse_power(&mut self) -> Result<ExprNode, String> {
        let lhs = self.parse_unary()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            let rhs = self.parse_power()?;
            return Ok(ExprNode::Binary {
                op: BinOp::Pow,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            });
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<ExprNode, String> {
        match self.peek() {
            Some(Token::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Token::Minus) => {
                self.next();
                Ok(ExprNode::UnaryNeg(Box::new(self.parse_unary()?)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<ExprNode, String> {
        match self.next() {
            Some(Token::Num(v)) => Ok(ExprNode::Const(v)),
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if !matches!(self.next(), Some(Token::RParen)) {
                        return Err(format!("函数 '{}' 缺少右括号 ')'", name));
                    }
                    validate_func(&name, args.len())?;
                    Ok(ExprNode::Call { func: name, args })
                } else {
                    match name.as_str() {
                        "pi" | "PI" => Ok(ExprNode::Const(std::f32::consts::PI)),
                        "e" | "E" => Ok(ExprNode::Const(std::f32::consts::E)),
                        _ => Ok(ExprNode::Var(name)),
                    }
                }
            }
            Some(Token::LParen) => {
                let inner = self.parse_expr()?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err("括号不匹配，缺少 ')'".to_string());
                }
                Ok(inner)
            }
            other => Err(format!("表达式语法错误: {:?}", other)),
        }
    }
}

fn validate_func(name: &str, argc: usize) -> Result<(), String> {
    let expected: &[usize] = match name {
        "sqrt" | "abs" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "log"
        | "log10" | "deg2rad" | "rad2deg" => &[1],
        "atan2" | "hypot" | "maximum" | "max" | "minimum" | "min" | "lpf" | "movavg"
        | "rms_win" => &[2],
        "diff" => &[1, 2],
        "clip" => &[3],
        _ => return Err(format!("不支持的函数 '{}'", name)),
    };
    if !expected.contains(&argc) {
        return Err(format!("函数 '{}' 参数个数错误 (收到 {})", name, argc));
    }
    Ok(())
}

pub fn compile_expression(expr: &str) -> Result<ExprNode, String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err("表达式不能为空".to_string());
    }
    let tokens = tokenize(trimmed)?;
    let mut parser = Parser { tokens, pos: 0 };
    let node = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        return Err("表达式尾部存在多余符号".to_string());
    }
    Ok(node)
}

pub fn eval_vectorized(
    node: &ExprNode,
    len: usize,
    env: &HashMap<String, Vec<f32>>,
) -> Result<Vec<f32>, String> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let mut out = eval_inner(node, len, env)?;
    for v in &mut out {
        if !v.is_finite() {
            *v = 0.0;
        }
    }
    Ok(out)
}

fn eval_inner(
    node: &ExprNode,
    len: usize,
    env: &HashMap<String, Vec<f32>>,
) -> Result<Vec<f32>, String> {
    match node {
        ExprNode::Const(c) => Ok(vec![*c; len]),
        ExprNode::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| format!("未找到变量 '{}'", name)),
        ExprNode::UnaryNeg(inner) => {
            let mut v = eval_inner(inner, len, env)?;
            for x in &mut v {
                *x = -*x;
            }
            Ok(v)
        }
        ExprNode::Binary { op, lhs, rhs } => {
            let mut a = eval_inner(lhs, len, env)?;
            let b = eval_inner(rhs, len, env)?;
            match op {
                BinOp::Add => {
                    for i in 0..len {
                        a[i] += b[i];
                    }
                }
                BinOp::Sub => {
                    for i in 0..len {
                        a[i] -= b[i];
                    }
                }
                BinOp::Mul => {
                    for i in 0..len {
                        a[i] *= b[i];
                    }
                }
                BinOp::Div => {
                    for i in 0..len {
                        a[i] = if b[i].abs() < 1e-12 {
                            0.0
                        } else {
                            a[i] / b[i]
                        };
                    }
                }
                BinOp::Rem => {
                    for i in 0..len {
                        a[i] = if b[i].abs() < 1e-12 {
                            0.0
                        } else {
                            a[i] % b[i]
                        };
                    }
                }
                BinOp::Pow => {
                    // Fast paths for ^2 and ^3
                    if matches!(rhs.as_ref(), ExprNode::Const(c) if (*c - 2.0).abs() < 1e-6) {
                        for x in &mut a {
                            *x = *x * *x;
                        }
                    } else {
                        for i in 0..len {
                            a[i] = a[i].powf(b[i]);
                        }
                    }
                }
            }
            Ok(a)
        }
        ExprNode::Call { func, args } => {
            let evaled: Result<Vec<Vec<f32>>, String> =
                args.iter().map(|a| eval_inner(a, len, env)).collect();
            let mut av = evaled?;
            match func.as_str() {
                "sqrt" => {
                    for x in &mut av[0] {
                        *x = x.max(0.0).sqrt();
                    }
                    Ok(av.remove(0))
                }
                "abs" => {
                    for x in &mut av[0] {
                        *x = x.abs();
                    }
                    Ok(av.remove(0))
                }
                "sin" => {
                    for x in &mut av[0] {
                        *x = x.sin();
                    }
                    Ok(av.remove(0))
                }
                "cos" => {
                    for x in &mut av[0] {
                        *x = x.cos();
                    }
                    Ok(av.remove(0))
                }
                "tan" => {
                    for x in &mut av[0] {
                        *x = x.tan();
                    }
                    Ok(av.remove(0))
                }
                "asin" => {
                    for x in &mut av[0] {
                        *x = x.clamp(-1.0, 1.0).asin();
                    }
                    Ok(av.remove(0))
                }
                "acos" => {
                    for x in &mut av[0] {
                        *x = x.clamp(-1.0, 1.0).acos();
                    }
                    Ok(av.remove(0))
                }
                "atan" => {
                    for x in &mut av[0] {
                        *x = x.atan();
                    }
                    Ok(av.remove(0))
                }
                "exp" => {
                    for x in &mut av[0] {
                        *x = x.exp();
                    }
                    Ok(av.remove(0))
                }
                "log" => {
                    for x in &mut av[0] {
                        *x = x.max(1e-12).ln();
                    }
                    Ok(av.remove(0))
                }
                "log10" => {
                    for x in &mut av[0] {
                        *x = x.max(1e-12).log10();
                    }
                    Ok(av.remove(0))
                }
                "deg2rad" => {
                    for x in &mut av[0] {
                        *x = x.to_radians();
                    }
                    Ok(av.remove(0))
                }
                "rad2deg" => {
                    for x in &mut av[0] {
                        *x = x.to_degrees();
                    }
                    Ok(av.remove(0))
                }
                "atan2" => {
                    let y = &av[0];
                    let x = &av[1];
                    let mut r = Vec::with_capacity(len);
                    for i in 0..len {
                        r.push(y[i].atan2(x[i]));
                    }
                    Ok(r)
                }
                "hypot" => {
                    let x = &av[0];
                    let y = &av[1];
                    let mut r = Vec::with_capacity(len);
                    for i in 0..len {
                        r.push(x[i].hypot(y[i]));
                    }
                    Ok(r)
                }
                "maximum" | "max" => {
                    let mut a = av.remove(0);
                    let b = &av[0];
                    for i in 0..len {
                        a[i] = a[i].max(b[i]);
                    }
                    Ok(a)
                }
                "minimum" | "min" => {
                    let mut a = av.remove(0);
                    let b = &av[0];
                    for i in 0..len {
                        a[i] = a[i].min(b[i]);
                    }
                    Ok(a)
                }
                "clip" => {
                    let mut x = av.remove(0);
                    let lo = &av[0];
                    let hi = &av[1];
                    for i in 0..len {
                        x[i] = x[i].clamp(lo[i], hi[i]);
                    }
                    Ok(x)
                }
                "lpf" => {
                    let x = &av[0];
                    let alpha = av[1][0].clamp(0.0001, 1.0);
                    let mut r = Vec::with_capacity(len);
                    let mut acc = x[0];
                    for &v in x {
                        acc += alpha * (v - acc);
                        r.push(acc);
                    }
                    Ok(r)
                }
                "movavg" => {
                    let x = &av[0];
                    let w = (av[1][0].round() as usize).clamp(1, 5000);
                    let mut r = Vec::with_capacity(len);
                    let mut sum = 0.0f64;
                    for i in 0..len {
                        sum += x[i] as f64;
                        if i >= w {
                            sum -= x[i - w] as f64;
                            r.push((sum / w as f64) as f32);
                        } else {
                            r.push((sum / (i + 1) as f64) as f32);
                        }
                    }
                    Ok(r)
                }
                "rms_win" => {
                    let x = &av[0];
                    let w = (av[1][0].round() as usize).clamp(1, 5000);
                    let mut r = Vec::with_capacity(len);
                    let mut sq_sum = 0.0f64;
                    for i in 0..len {
                        let v = x[i] as f64;
                        sq_sum += v * v;
                        if i >= w {
                            let old = x[i - w] as f64;
                            sq_sum = (sq_sum - old * old).max(0.0);
                            r.push((sq_sum / w as f64).sqrt() as f32);
                        } else {
                            r.push((sq_sum / (i + 1) as f64).sqrt() as f32);
                        }
                    }
                    Ok(r)
                }
                "diff" => {
                    let x = &av[0];
                    let dt = if av.len() > 1 {
                        av[1][0].max(1e-9)
                    } else {
                        1.0
                    };
                    let mut r = Vec::with_capacity(len);
                    r.push(0.0);
                    for i in 1..len {
                        r.push((x[i] - x[i - 1]) / dt);
                    }
                    Ok(r)
                }
                _ => Err(format!("未知函数 '{}'", func)),
            }
        }
    }
}
