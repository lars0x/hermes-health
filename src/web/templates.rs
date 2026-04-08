use std::sync::Arc;

use minijinja::{path_loader, Environment};

use crate::error::HermesError;

#[derive(Clone)]
pub struct TemplateEngine {
    env: Arc<Environment<'static>>,
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine {
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_loader(path_loader("templates"));

        // Add custom filters
        env.add_filter("format_value", format_value);
        env.add_filter("trend_arrow", trend_arrow);
        env.add_filter("status_color", status_color);
        env.add_filter("range_display", range_display);

        Self { env: Arc::new(env) }
    }

    pub fn render(
        &self,
        template_name: &str,
        ctx: minijinja::Value,
    ) -> Result<String, HermesError> {
        let tmpl = self
            .env
            .get_template(template_name)
            .map_err(|e| HermesError::Config(format!("Template error: {e}")))?;
        tmpl.render(ctx)
            .map_err(|e| HermesError::Config(format!("Render error: {e}")))
    }
}

// Custom filters

fn format_value(value: f64, precision: Option<i32>) -> String {
    let prec = precision.unwrap_or(0) as usize;
    format!("{:.prec$}", value)
}

fn trend_arrow(direction: &str) -> &'static str {
    match direction {
        "increasing" => "\u{25B2}", // filled up triangle
        "decreasing" => "\u{25BC}", // filled down triangle
        "stable" => "\u{25B6}",     // filled right triangle
        _ => "",
    }
}

fn status_color(status: &str) -> &'static str {
    match status {
        "worsening" | "out_of_reference" => "text-red",
        "improving" => "text-green",
        "suboptimal" => "text-amber",
        "stable" | "in_range" => "text-green",
        "insufficient_data" => "text-secondary",
        _ => "",
    }
}

fn range_display(low: Option<f64>, high: Option<f64>) -> String {
    match (low, high) {
        (Some(l), Some(h)) => format!("{}-{}", l, h),
        (Some(l), None) => format!("{}+", l),
        (None, Some(h)) => format!("<{}", h),
        (None, None) => "-".to_string(),
    }
}
