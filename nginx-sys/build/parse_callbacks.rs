use core::cell::Cell;

use bindgen::callbacks::{ItemInfo, MacroParsingBehavior};

#[derive(Debug, Default)]
pub struct NginxCallbacks {
    off_t_is_macro: Cell<bool>,
}

impl bindgen::callbacks::ParseCallbacks for NginxCallbacks {
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        // Detect and rename (in item_name) libc types defined as a macro instead of alias.
        if name == "off_t" {
            self.off_t_is_macro.set(true);
        }

        MacroParsingBehavior::Default
    }

    fn item_name(&self, item_info: ItemInfo) -> Option<String> {
        match item_info.name {
            "__off_t" if self.off_t_is_macro.get() => Some("off_t".to_string()),
            _ => None,
        }
    }
}
