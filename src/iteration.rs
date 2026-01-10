use thiserror::Error;

#[derive(Error, Debug)]
pub enum IterationError {
    #[error("Invalid iteration syntax: {0}")]
    InvalidSyntax(String),
    #[error("Data path not found: {0}")]
    DataPathNotFound(String),
}

#[derive(Debug, Clone)]
pub struct IterationInfo {
    pub var: String,
    pub expr: String,
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub enum IterationPattern {
    Simple(IterationInfo),
    Nested(Vec<IterationInfo>),
    Array(Vec<IterationPattern>),
    Union(Vec<IterationPattern>),
}

pub struct IterationEvaluator;

impl IterationEvaluator {
    /// Parses a simple iteration expression like "item in items" or "item in items if item.enabled"
    pub fn parse_simple(expr: &str) -> Result<IterationInfo, IterationError> {
        // Check if there's an "if" condition
        let (iter_part, condition) = if expr.contains(" if ") {
            let parts: Vec<&str> = expr.splitn(2, " if ").collect();
            (parts[0], Some(parts[1].trim().to_string()))
        } else {
            (expr, None)
        };

        // Parse "var in expr"
        let parts: Vec<&str> = iter_part.split(" in ").collect();
        if parts.len() != 2 {
            return Err(IterationError::InvalidSyntax(expr.to_string()));
        }

        Ok(IterationInfo {
            var: parts[0].trim().to_string(),
            expr: parts[1].trim().to_string(),
            condition,
        })
    }

    /// Parses a nested iteration expression like "parent in parents >> child in parent.children"
    pub fn parse_nested(expr: &str) -> Result<Vec<IterationInfo>, IterationError> {
        let parts: Vec<&str> = expr.split(">>").collect();
        let mut iterations = Vec::new();

        for part in parts {
            iterations.push(Self::parse_simple(part.trim())?);
        }

        Ok(iterations)
    }

    /// Parses a union iteration expression like "v1 in c1 | v2 in c2"
    pub fn parse_union(expr: &str) -> Result<Vec<IterationPattern>, IterationError> {
        let parts: Vec<&str> = expr.split('|').collect();
        let mut patterns = Vec::new();

        for part in parts {
            let trimmed = part.trim();
            // Each part can be simple or nested but not another union
            if trimmed.contains(">>") {
                patterns.push(IterationPattern::Nested(Self::parse_nested(trimmed)?));
            } else {
                patterns.push(IterationPattern::Simple(Self::parse_simple(trimmed)?));
            }
        }

        Ok(patterns)
    }

    /// Parses any iteration pattern (simple, nested, union, or array)
    pub fn parse(expr: &str) -> Result<IterationPattern, IterationError> {
        // Check for union iteration (|) - but not inside conditions
        // We need to be careful not to split on | inside "if" conditions
        if Self::has_union_operator(expr) {
            Ok(IterationPattern::Union(Self::parse_union(expr)?))
        } else if expr.contains(">>") {
            Ok(IterationPattern::Nested(Self::parse_nested(expr)?))
        } else {
            Ok(IterationPattern::Simple(Self::parse_simple(expr)?))
        }
    }

    /// Check if expression has union operator at top level (not inside if condition)
    fn has_union_operator(expr: &str) -> bool {
        // Split by " if " first to isolate the iteration part from condition
        let iter_part = if expr.contains(" if ") {
            expr.splitn(2, " if ").next().unwrap_or(expr)
        } else {
            expr
        };
        // Check if there's a | in the iteration part (outside of nested >>)
        // Simple heuristic: if there's a | not preceded by >> context
        iter_part.contains('|')
    }

    /// Evaluates a data path expression (e.g., "dd.services" -> "/services")
    pub fn evaluate_path(expr: &str) -> String {
        // Convert dot notation to JSON pointer
        let path = expr.trim();

        // Remove "dd." prefix if present
        let path = if path.starts_with("dd.") {
            &path[3..]
        } else {
            path
        };

        format!("/{}", path.replace('.', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let result = IterationEvaluator::parse_simple("service in services").unwrap();
        assert_eq!(result.var, "service");
        assert_eq!(result.expr, "services");
        assert!(result.condition.is_none());
    }

    #[test]
    fn test_parse_simple_with_condition() {
        let result =
            IterationEvaluator::parse_simple("service in services if service.enabled").unwrap();
        assert_eq!(result.var, "service");
        assert_eq!(result.expr, "services");
        assert_eq!(result.condition, Some("service.enabled".to_string()));
    }

    #[test]
    fn test_parse_nested() {
        let result =
            IterationEvaluator::parse_nested("module in modules >> component in module.components")
                .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].var, "module");
        assert_eq!(result[0].expr, "modules");
        assert_eq!(result[1].var, "component");
        assert_eq!(result[1].expr, "module.components");
    }

    #[test]
    fn test_parse_union() {
        let result = IterationEvaluator::parse_union("a in as | b in bs").unwrap();
        assert_eq!(result.len(), 2);
        match &result[0] {
            IterationPattern::Simple(info) => {
                assert_eq!(info.var, "a");
                assert_eq!(info.expr, "as");
            }
            _ => panic!("Expected Simple pattern"),
        }
        match &result[1] {
            IterationPattern::Simple(info) => {
                assert_eq!(info.var, "b");
                assert_eq!(info.expr, "bs");
            }
            _ => panic!("Expected Simple pattern"),
        }
    }

    #[test]
    fn test_parse_detects_union() {
        let result = IterationEvaluator::parse("x in xs | y in ys").unwrap();
        match result {
            IterationPattern::Union(patterns) => assert_eq!(patterns.len(), 2),
            _ => panic!("Expected Union pattern"),
        }
    }

    #[test]
    fn test_evaluate_path() {
        assert_eq!(
            IterationEvaluator::evaluate_path("dd.services"),
            "/services"
        );
        assert_eq!(IterationEvaluator::evaluate_path("services"), "/services");
        assert_eq!(
            IterationEvaluator::evaluate_path("dd.modules.components"),
            "/modules/components"
        );
    }
}
