//! The `=` calculator: + - * / ^ and brackets.

/// Evaluates + - * / % ^ with parentheses and decimals.
pub fn calc(expr: &str) -> Option<String> {
    let tokens: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).collect();
    if tokens.is_empty() {
        return None;
    }
    let mut p = Parser { t: &tokens, i: 0 };
    let v = p.sum()?;
    if p.i != tokens.len() || !v.is_finite() {
        return None;
    }
    let s = if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    Some(s)
}

pub(super) struct Parser<'a> {
    t: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.t.get(self.i).copied()
    }
    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        while let Some(c) = self.peek() {
            match c {
                '+' => {
                    self.i += 1;
                    v += self.product()?;
                }
                '-' => {
                    self.i += 1;
                    v -= self.product()?;
                }
                _ => break,
            }
        }
        Some(v)
    }
    fn product(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(c) = self.peek() {
            match c {
                '*' | 'x' | '×' => {
                    self.i += 1;
                    v *= self.power()?;
                }
                '/' | '÷' => {
                    self.i += 1;
                    v /= self.power()?;
                }
                '%' => {
                    self.i += 1;
                    v %= self.power()?;
                }
                _ => break,
            }
        }
        Some(v)
    }
    fn power(&mut self) -> Option<f64> {
        let b = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let e = self.power()?;
            return Some(b.powf(e));
        }
        Some(b)
    }
    fn unary(&mut self) -> Option<f64> {
        if self.peek() == Some('-') {
            self.i += 1;
            return Some(-self.unary()?);
        }
        self.atom()
    }
    fn atom(&mut self) -> Option<f64> {
        if self.peek() == Some('(') {
            self.i += 1;
            let v = self.sum()?;
            if self.peek() != Some(')') {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.i += 1;
        }
        self.t[start..self.i]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    }
}
