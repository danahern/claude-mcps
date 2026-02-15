//! Template constants and rendering for the `create_app` tool

pub const TEMPLATE_CMAKE: &str = r#"cmake_minimum_required(VERSION 3.20.0)

# Shared library config overlays
{{OVERLAY_LINES}}

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project({{APP_NAME}})

target_sources(app PRIVATE src/main.c)
"#;

pub const TEMPLATE_PRJ_CONF: &str = r#"# Logging
CONFIG_LOG=y
CONFIG_LOG_DEFAULT_LEVEL=3

# Stack sizes
CONFIG_MAIN_STACK_SIZE=2048
CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=2048

# Reboot support
CONFIG_REBOOT=y
"#;

pub const TEMPLATE_MAIN_C: &str = r#"#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER({{APP_NAME}}, LOG_LEVEL_INF);

int main(void)
{
	LOG_INF("{{APP_NAME}} booted");

	while (1) {
		k_sleep(K_FOREVER);
	}

	return 0;
}
"#;

pub const TEMPLATE_MANIFEST: &str = r#"description: "{{DESCRIPTION}}"
boards:
{{BOARD_LINES}}
libraries:
{{LIBRARY_LINES}}
template: {{TEMPLATE}}
"#;

/// Replace `{{PLACEHOLDER}}` patterns in a template string.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_basic() {
        let out = render("Hello {{NAME}}!", &[("NAME", "world")]);
        assert_eq!(out, "Hello world!");
    }

    #[test]
    fn test_render_multiple() {
        let out = render("{{A}} + {{B}}", &[("A", "1"), ("B", "2")]);
        assert_eq!(out, "1 + 2");
    }

    #[test]
    fn test_render_repeated() {
        let out = render("{{X}} and {{X}}", &[("X", "y")]);
        assert_eq!(out, "y and y");
    }

    #[test]
    fn test_render_no_match() {
        let out = render("no placeholders", &[("X", "y")]);
        assert_eq!(out, "no placeholders");
    }

    #[test]
    fn test_template_cmake_renders() {
        let out = render(TEMPLATE_CMAKE, &[
            ("APP_NAME", "my_app"),
            ("OVERLAY_LINES", "list(APPEND OVERLAY_CONFIG \"test.conf\")"),
        ]);
        assert!(out.contains("project(my_app)"));
        assert!(out.contains("test.conf"));
    }

    #[test]
    fn test_template_main_c_renders() {
        let out = render(TEMPLATE_MAIN_C, &[("APP_NAME", "sensor_hub")]);
        assert!(out.contains("LOG_MODULE_REGISTER(sensor_hub"));
        assert!(out.contains("sensor_hub booted"));
    }
}
