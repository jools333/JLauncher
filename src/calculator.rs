use evalexpr::*;

#[derive(Debug, Clone)]
pub struct CalcResult {
    pub expression: String,
    pub formatted_result: String,
}

pub struct Calculator {
    context: HashMapContext,
}

impl Calculator {
    pub fn new() -> Self {
        let mut context = HashMapContext::new();
        // Provide standard mathematical constants
        let _ = context.set_value("pi".into(), Value::from_float(std::f64::consts::PI));
        let _ = context.set_value("e".into(), Value::from_float(std::f64::consts::E));
        Self { context }
    }

    pub fn evaluate(&self, input: &str) -> Option<CalcResult> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Check if query is prefixed with '=' or contains common math tokens
        let (expr_str, is_explicit) = if let Some(stripped) = trimmed.strip_prefix('=') {
            (stripped.trim(), true)
        } else {
            (trimmed, false)
        };

        if expr_str.is_empty() {
            return None;
        }

        // If not explicit, only attempt calculation if it looks like math
        if !is_explicit && !Self::looks_like_math(expr_str) {
            return None;
        }

        // Normalize some common patterns:
        // Replace "X% of Y" with "(Y * X / 100)"
        let normalized = Self::normalize_expression(expr_str);

        match eval_with_context(&normalized, &self.context) {
            Ok(Value::Float(f)) => {
                if f.is_nan() || f.is_infinite() {
                    None
                } else {
                    Some(CalcResult {
                        expression: expr_str.to_string(),
                        formatted_result: Self::format_float(f),
                    })
                }
            }
            Ok(Value::Int(i)) => Some(CalcResult {
                expression: expr_str.to_string(),
                formatted_result: format!("{}", i),
            }),
            Ok(Value::Boolean(b)) if is_explicit => Some(CalcResult {
                expression: expr_str.to_string(),
                formatted_result: format!("{}", b),
            }),
            _ => None,
        }
    }

    fn looks_like_math(s: &str) -> bool {
        // Must contain at least one math operator or known math function/constant
        let has_digit = s.chars().any(|c| c.is_ascii_digit());
        let has_operator = s.contains('+')
            || s.contains('-')
            || s.contains('*')
            || s.contains('/')
            || s.contains('^')
            || s.contains('%');
        let has_func = s.contains("sqrt")
            || s.contains("sin")
            || s.contains("cos")
            || s.contains("tan")
            || s.contains("abs")
            || s.contains("log")
            || s.contains("pow")
            || s.contains("pi");

        has_digit && (has_operator || has_func)
    }

    fn normalize_expression(s: &str) -> String {
        // Check for "X% of Y"
        if let Some(pos) = s.to_lowercase().find("% of ") {
            let left = s[..pos].trim();
            let right = s[pos + 5..].trim();
            if !left.is_empty() && !right.is_empty() {
                return format!("({} * {} / 100.0)", right, left);
            }
        }

        let mut res = s.to_string();
        for func in &["sqrt", "sin", "cos", "tan", "abs", "log", "exp"] {
            let pattern = format!("{}(", func);
            let target = format!("math::{}(", func);
            if res.contains(&pattern) && !res.contains(&target) {
                res = res.replace(&pattern, &target);
            }
        }
        res
    }

    fn format_float(f: f64) -> String {
        if f.fract() == 0.0 && f.abs() < 1e15 {
            format!("{:.0}", f)
        } else {
            // Print up to 6 decimal places, trimming trailing zeros
            let s = format!("{:.6}", f);
            let s = s.trim_end_matches('0');
            s.trim_end_matches('.').to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_basic_arithmetic() {
        let calc = Calculator::new();
        let res = calc.evaluate("2 + 2 * 3").unwrap();
        assert_eq!(res.formatted_result, "8");

        let res = calc.evaluate("(100 - 20) / 4").unwrap();
        assert_eq!(res.formatted_result, "20");
    }

    #[test]
    fn test_calculator_percentages() {
        let calc = Calculator::new();
        let res = calc.evaluate("25% of 200").unwrap();
        assert_eq!(res.formatted_result, "50");
    }

    #[test]
    fn test_calculator_constants_and_functions() {
        let calc = Calculator::new();
        let res = calc.evaluate("math::sqrt(144)").unwrap();
        assert_eq!(res.formatted_result, "12");

        let res = calc.evaluate("= pi").unwrap();
        assert!(res.formatted_result.starts_with("3.1415"));
    }

    #[test]
    fn test_calculator_rejects_plain_text() {
        let calc = Calculator::new();
        assert!(calc.evaluate("google chrome").is_none());
        assert!(calc.evaluate("firefox").is_none());
        assert!(calc.evaluate("hello world").is_none());
    }

    #[test]
    fn test_calculator_explicit_prefix() {
        let calc = Calculator::new();
        let res = calc.evaluate("= 42").unwrap();
        assert_eq!(res.formatted_result, "42");
    }
}
