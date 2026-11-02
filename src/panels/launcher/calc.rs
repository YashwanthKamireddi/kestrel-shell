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

#[cfg(test)]
mod tests {
    use super::calc;

    fn is(expr: &str, want: &str) {
        assert_eq!(calc(expr).as_deref(), Some(want), "{expr}");
    }

    #[test]
    fn the_same_level_goes_left_to_right() {
        is("10-3-2", "5");
        is("100/10/2", "5");
        is("2*3/4", "1.5");
        is("17%5*2", "4");
    }

    #[test]
    fn powers_go_right_to_left_and_may_be_negative() {
        is("2^3^2", "512");
        is("2^-1", "0.5");
        is("4^0.5", "2");
        is("(2^3)^2", "64");
    }

    #[test]
    fn signs() {
        is("--3", "3");
        is("-(2+3)", "-5");
        is("2*-3", "-6");
        is("3--1", "4");
    }

    #[test]
    fn every_way_of_writing_times_and_divide() {
        is("3x4", "12");
        is("3×4", "12");
        is("8÷2", "4");
        is("10%3", "1");
        is("7.5%2", "1.5");
    }

    #[test]
    fn decimals_read_as_people_write_them() {
        is("0.1+0.2", "0.3");
        is(".5*2", "1");
        is("1/3", "0.3333333333");
        is("  1 +  2 ", "3");
        is("10^20", "100000000000000000000");
    }

    #[test]
    fn nonsense_has_no_answer() {
        for bad in [
            "", "   ", "1/0", "0/0", "(1+2", "1+2)", "()", "1..2", "1e3", "2(3)", "*2", "1+", "2^",
            "-",
        ] {
            assert_eq!(calc(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn deep_brackets() {
        let deep = format!("{}1{}", "(".repeat(50), ")".repeat(50));
        is(&deep, "1");
    }
}
