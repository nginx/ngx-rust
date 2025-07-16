use std::collections::HashMap;

use bindgen::callbacks::{DeriveTrait, ImplementsTrait};
use bitflags::bitflags;

bitflags! {
    /// Efficient encoding for bindgen type information enums
    #[derive(Clone, Debug)]
    pub struct TypeFlags: u8 {
        const COPY = 1;
        const DEBUG = 1 << 1;
        const DEFAULT = 1 << 2;
        const HASH = 1 << 3;
        const PARTIAL_ORD_OR_PARTIAL_EQ = 1 << 4;
    }
}

impl TypeFlags {
    pub fn implements(&self, t: DeriveTrait) -> bool {
        match t {
            DeriveTrait::Copy => self.contains(Self::COPY),
            DeriveTrait::Debug => self.contains(Self::DEBUG),
            DeriveTrait::Default => self.contains(Self::DEFAULT),
            DeriveTrait::Hash => self.contains(Self::HASH),
            DeriveTrait::PartialEqOrPartialOrd => self.contains(Self::PARTIAL_ORD_OR_PARTIAL_EQ),
        }
    }
}

#[derive(Debug)]
struct Crate<'a> {
    name: &'a str,
    types: HashMap<&'a str, TypeFlags>,
}

impl<'a> Crate<'a> {
    pub fn new(name: &'a str, types: impl IntoIterator<Item = (&'a str, TypeFlags)>) -> Self {
        Self {
            name,
            types: HashMap::from_iter(types),
        }
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().cloned()
    }

    pub fn uses(&self) -> Option<String> {
        if self.types.is_empty() {
            return None;
        }

        Some(format!(
            r#"
#[allow(unused_imports)]
pub use {}::{{{}}};
"#,
            self.name,
            self.type_names().collect::<Vec<_>>().join(",")
        ))
    }
}

#[derive(Debug, Default)]
pub struct NgxBindgenCallbacks<'a>(Vec<Crate<'a>>);

impl<'a> NgxBindgenCallbacks<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_external_types(
        &mut self,
        source: &'a str,
        types: impl IntoIterator<Item = (&'a str, TypeFlags)>,
    ) {
        if let Some(c) = self.0.iter_mut().find(|c| c.name == source) {
            c.types.extend(types)
        } else {
            self.0.push(Crate::new(source, types));
        }
    }

    fn find(&self, name: &str) -> Option<(&Crate, &str, &TypeFlags)> {
        for c in &self.0[..] {
            for (key, value) in c.types.iter() {
                if *key == name {
                    return Some((c, *key, value));
                }
            }
        }
        None
    }

    fn blocklist(&self) -> String {
        self.0
            .iter()
            .flat_map(Crate::type_names)
            .collect::<Vec<_>>()
            .join("|")
    }

    fn uses(&self) -> String {
        self.0
            .iter()
            .flat_map(Crate::uses)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn add_to_builder(self, mut builder: bindgen::Builder) -> bindgen::Builder
    where
        'a: 'static,
    {
        let blocklist = self.blocklist();
        if !blocklist.is_empty() {
            builder = builder.blocklist_type(blocklist);
        }

        let uses = self.uses();
        if !uses.is_empty() {
            builder = builder.raw_line(uses);
        }

        builder.parse_callbacks(Box::new(self))
    }
}

impl<'a> bindgen::callbacks::ParseCallbacks for NgxBindgenCallbacks<'a> {
    fn blocklisted_type_implements_trait(
        &self,
        name: &str,
        derive_trait: DeriveTrait,
    ) -> Option<ImplementsTrait> {
        let parts = name.split_ascii_whitespace().collect::<Vec<_>>();
        let type_name = match &parts[..] {
            ["const", "struct", n] => n,
            ["const", n] => n,
            ["struct", n] => n,
            [n] => n,
            _ => panic!("unhandled blocklisted type: {name}"),
        };

        if self.find(type_name)?.2.implements(derive_trait) {
            return Some(ImplementsTrait::Yes);
        }
        None
    }
}
