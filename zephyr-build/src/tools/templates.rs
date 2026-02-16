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
{{ADDON_KCONFIG}}"#;

pub const TEMPLATE_MAIN_C: &str = r#"#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
{{ADDON_INCLUDES}}
LOG_MODULE_REGISTER({{APP_NAME}}, LOG_LEVEL_INF);
{{ADDON_GLOBALS}}
int main(void)
{
{{ERR_DECL}}	LOG_INF("{{APP_NAME}} booted");
{{ADDON_INIT}}
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

use super::types::AddonManifest;

/// Collected addon code sections ready for template injection.
pub struct AddonCodeSections {
    pub kconfig: String,
    pub includes: String,
    pub globals: String,
    pub err_decl: String,
    pub init: String,
}

/// Merge multiple addon manifests into collected code sections.
pub fn merge_addon_code(addons: &[AddonManifest], app_name: &str) -> AddonCodeSections {
    let mut kconfig_parts = Vec::new();
    let mut include_parts = Vec::new();
    let mut global_parts = Vec::new();
    let mut init_parts = Vec::new();

    for addon in addons {
        if let Some(k) = &addon.kconfig {
            let rendered = render(k.trim_end(), &[("APP_NAME", app_name)]);
            kconfig_parts.push(rendered);
        }
        if let Some(i) = &addon.includes {
            include_parts.push(i.trim_end().to_string());
        }
        if let Some(g) = &addon.globals {
            global_parts.push(g.trim_end().to_string());
        }
        if let Some(init) = &addon.init {
            // Indent each non-empty line with a tab
            let indented: String = init
                .trim_end()
                .lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        String::new()
                    } else {
                        format!("\t{}", line)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            init_parts.push(indented);
        }
    }

    let has_init = !init_parts.is_empty();

    let kconfig = if kconfig_parts.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", kconfig_parts.join("\n\n"))
    };

    let includes = if include_parts.is_empty() {
        String::new()
    } else {
        format!("{}\n", include_parts.join("\n"))
    };

    let globals = if global_parts.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", global_parts.join("\n\n"))
    };

    let err_decl = if has_init {
        "\tint err;\n\n".to_string()
    } else {
        String::new()
    };

    let init = if init_parts.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", init_parts.join("\n\n"))
    };

    AddonCodeSections {
        kconfig,
        includes,
        globals,
        err_decl,
        init,
    }
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
        let out = render(TEMPLATE_MAIN_C, &[
            ("APP_NAME", "sensor_hub"),
            ("ADDON_INCLUDES", ""),
            ("ADDON_GLOBALS", ""),
            ("ERR_DECL", ""),
            ("ADDON_INIT", ""),
        ]);
        assert!(out.contains("LOG_MODULE_REGISTER(sensor_hub"));
        assert!(out.contains("sensor_hub booted"));
        // No unused `int err;` when no addons
        assert!(!out.contains("int err;"));
    }

    #[test]
    fn test_template_prj_conf_renders_with_addon_kconfig() {
        let out = render(TEMPLATE_PRJ_CONF, &[
            ("ADDON_KCONFIG", "\n# BLE\nCONFIG_BT=y\n"),
        ]);
        assert!(out.contains("CONFIG_REBOOT=y"));
        assert!(out.contains("CONFIG_BT=y"));
    }

    #[test]
    fn test_template_prj_conf_renders_clean_without_addons() {
        let out = render(TEMPLATE_PRJ_CONF, &[("ADDON_KCONFIG", "")]);
        assert!(out.contains("CONFIG_REBOOT=y"));
        assert!(out.ends_with("CONFIG_REBOOT=y\n"));
    }

    #[test]
    fn test_merge_addon_code_empty() {
        let sections = merge_addon_code(&[], "my_app");
        assert_eq!(sections.kconfig, "");
        assert_eq!(sections.includes, "");
        assert_eq!(sections.globals, "");
        assert_eq!(sections.err_decl, "");
        assert_eq!(sections.init, "");
    }

    #[test]
    fn test_merge_addon_code_single() {
        let addon = AddonManifest {
            name: "ble".to_string(),
            description: "BLE".to_string(),
            depends: vec![],
            kconfig: Some("CONFIG_BT=y\nCONFIG_BT_DEVICE_NAME=\"{{APP_NAME}}\"".to_string()),
            includes: Some("#include <zephyr/bluetooth/bluetooth.h>".to_string()),
            globals: Some("static struct bt_conn *current_conn;".to_string()),
            init: Some("err = bt_enable(NULL);".to_string()),
        };
        let sections = merge_addon_code(&[addon], "test_app");

        assert!(sections.kconfig.contains("CONFIG_BT=y"));
        assert!(sections.kconfig.contains("CONFIG_BT_DEVICE_NAME=\"test_app\""));
        assert!(sections.includes.contains("#include <zephyr/bluetooth/bluetooth.h>"));
        assert!(sections.globals.contains("static struct bt_conn *current_conn;"));
        assert_eq!(sections.err_decl, "\tint err;\n\n");
        assert!(sections.init.contains("\terr = bt_enable(NULL);"));
    }

    #[test]
    fn test_merge_addon_code_multiple() {
        let ble = AddonManifest {
            name: "ble".to_string(),
            description: "BLE".to_string(),
            depends: vec![],
            kconfig: Some("CONFIG_BT=y".to_string()),
            includes: Some("#include <bt.h>".to_string()),
            globals: None,
            init: Some("err = bt_enable(NULL);".to_string()),
        };
        let wifi = AddonManifest {
            name: "wifi".to_string(),
            description: "WiFi".to_string(),
            depends: vec![],
            kconfig: Some("CONFIG_WIFI=y".to_string()),
            includes: Some("#include <wifi.h>".to_string()),
            globals: Some("static bool wifi_connected;".to_string()),
            init: Some("err = wifi_connect();".to_string()),
        };
        let sections = merge_addon_code(&[ble, wifi], "combo");

        assert!(sections.kconfig.contains("CONFIG_BT=y"));
        assert!(sections.kconfig.contains("CONFIG_WIFI=y"));
        assert!(sections.includes.contains("#include <bt.h>"));
        assert!(sections.includes.contains("#include <wifi.h>"));
        assert!(sections.globals.contains("static bool wifi_connected;"));
        assert_eq!(sections.err_decl, "\tint err;\n\n");
        assert!(sections.init.contains("\terr = bt_enable(NULL);"));
        assert!(sections.init.contains("\terr = wifi_connect();"));
    }

    #[test]
    fn test_merge_addon_no_init_no_err_decl() {
        let addon = AddonManifest {
            name: "test".to_string(),
            description: "test".to_string(),
            depends: vec![],
            kconfig: Some("CONFIG_X=y".to_string()),
            includes: None,
            globals: None,
            init: None,
        };
        let sections = merge_addon_code(&[addon], "app");
        assert_eq!(sections.err_decl, "");
        assert_eq!(sections.init, "");
    }

    #[test]
    fn test_merge_addon_multiline_init_indented() {
        let addon = AddonManifest {
            name: "test".to_string(),
            description: "test".to_string(),
            depends: vec![],
            kconfig: None,
            includes: None,
            globals: None,
            init: Some("err = bt_enable(NULL);\nif (err) {\n\tLOG_ERR(\"failed\");\n}".to_string()),
        };
        let sections = merge_addon_code(&[addon], "app");
        // init is wrapped: "\n<indented code>\n"
        assert!(sections.init.contains("\terr = bt_enable(NULL);"));
        assert!(sections.init.contains("\tif (err) {"));
        assert!(sections.init.contains("\t\tLOG_ERR(\"failed\");")); // original \t + our \t
        assert!(sections.init.contains("\t}"));
    }

    #[test]
    fn test_full_template_with_addon() {
        let addon = AddonManifest {
            name: "ble".to_string(),
            description: "BLE".to_string(),
            depends: vec![],
            kconfig: Some("CONFIG_BT=y".to_string()),
            includes: Some("#include <zephyr/bluetooth/bluetooth.h>".to_string()),
            globals: Some("static struct bt_conn *conn;".to_string()),
            init: Some("err = bt_enable(NULL);".to_string()),
        };
        let sections = merge_addon_code(&[addon], "my_ble_app");

        let main_c = render(TEMPLATE_MAIN_C, &[
            ("APP_NAME", "my_ble_app"),
            ("ADDON_INCLUDES", &sections.includes),
            ("ADDON_GLOBALS", &sections.globals),
            ("ERR_DECL", &sections.err_decl),
            ("ADDON_INIT", &sections.init),
        ]);

        assert!(main_c.contains("#include <zephyr/bluetooth/bluetooth.h>"));
        assert!(main_c.contains("static struct bt_conn *conn;"));
        assert!(main_c.contains("int err;"));
        assert!(main_c.contains("\terr = bt_enable(NULL);"));
        assert!(main_c.contains("LOG_MODULE_REGISTER(my_ble_app"));
    }
}
