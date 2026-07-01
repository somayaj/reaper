use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportMap {
    pub explicit: HashMap<String, String>,
    pub wildcards: Vec<String>,
}

impl ImportMap {
    pub fn fqcn_for_simple(&self, symbol: &str) -> Option<&str> {
        self.explicit.get(symbol).map(String::as_str)
    }
}

/// Non-wildcard type import FQCNs (for javac companion compilation).
pub fn type_import_fqcns(map: &ImportMap) -> Vec<String> {
    map.explicit.values().cloned().collect()
}
