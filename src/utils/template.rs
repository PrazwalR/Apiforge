use crate::error::{ApiForgError, Result};
use std::collections::HashMap;
use tera::{Context, Tera};

pub struct TemplateEngine {
    tera: Tera,
}

impl TemplateEngine {
    pub fn new() -> Self {
        Self {
            tera: Tera::default(),
        }
    }

    pub fn render(&mut self, template: &str, context: &HashMap<String, String>) -> Result<String> {
        let mut tera_context = Context::new();
        for (key, value) in context {
            tera_context.insert(key, value);
        }

        self.tera
            .render_str(template, &tera_context)
            .map_err(|e| ApiForgError::Config(format!("Template rendering failed: {}", e)))
    }

    pub fn render_simple(&mut self, template: &str, key: &str, value: &str) -> Result<String> {
        let mut context = HashMap::new();
        context.insert(key.to_string(), value.to_string());
        self.render(template, &context)
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let mut engine = TemplateEngine::new();
        let result = engine
            .render_simple("Hello {{ version }}!", "version", "1.2.3")
            .unwrap();
        assert_eq!(result, "Hello 1.2.3!");
    }

    #[test]
    fn test_render_multiple() {
        let mut engine = TemplateEngine::new();
        let mut context = HashMap::new();
        context.insert("project".to_string(), "myapp".to_string());
        context.insert("version".to_string(), "1.0.0".to_string());

        let result = engine
            .render("Deploying {{ project }} v{{ version }}", &context)
            .unwrap();
        assert_eq!(result, "Deploying myapp v1.0.0");
    }
}
