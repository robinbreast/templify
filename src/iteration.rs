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

    /// Parses any iteration pattern (simple, nested, or array)
    pub fn parse(expr: &str) -> Result<IterationPattern, IterationError> {
        // Check for nested iteration
        if expr.contains(">>") {
            Ok(IterationPattern::Nested(Self::parse_nested(expr)?))
        } else {
            Ok(IterationPattern::Simple(Self::parse_simple(expr)?))
        }
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
        let result = IterationEvaluator::parse_simple("service in services if service.enabled").unwrap();
        assert_eq!(result.var, "service");
        assert_eq!(result.expr, "services");
        assert_eq!(result.condition, Some("service.enabled".to_string()));
    }

    #[test]
    fn test_parse_nested() {
        let result = IterationEvaluator::parse_nested("module in modules >> component in module.components").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].var, "module");
        assert_eq!(result[0].expr, "modules");
        assert_eq!(result[1].var, "component");
        assert_eq!(result[1].expr, "module.components");
    }

    #[test]
    fn test_evaluate_path() {
        assert_eq!(IterationEvaluator::evaluate_path("dd.services"), "/services");
        assert_eq!(IterationEvaluator::evaluate_path("services"), "/services");
        assert_eq!(IterationEvaluator::evaluate_path("dd.modules.components"), "/modules/components");
    }
}
