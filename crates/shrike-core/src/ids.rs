//! Interned identifiers.
//!
//! All identifiers are `u32` handles into typed arenas. They are scoped to a
//! single snapshot and are never serialized as numbers; the wire format uses
//! stable string keys (for example `src/lib/format.ts#formatInvoice`).

use std::fmt;

macro_rules! id_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            #[must_use]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                Self(value)
            }
        }

        impl From<usize> for $name {
            fn from(value: usize) -> Self {
                Self(value as u32)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", $label, self.0)
            }
        }
    };
}

id_type!(FileId, "file");
id_type!(SymbolId, "symbol");
id_type!(ModuleId, "module");
id_type!(EntryId, "entry");
id_type!(EdgeId, "edge");
id_type!(SccId, "scc");

/// Stable key for a symbol: `relative/path.ts#name`.
#[must_use]
pub fn symbol_key(file: &str, name: &str) -> String {
    format!("{file}#{name}")
}

/// Stable key for a module node: `relative/path.ts#__module__`.
pub const MODULE_SYMBOL_NAME: &str = "__module__";

#[must_use]
pub fn module_symbol_key(file: &str) -> String {
    symbol_key(file, MODULE_SYMBOL_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_displayable() {
        assert_eq!(FileId(7).index(), 7);
        assert_eq!(SymbolId::from(3usize).0, 3);
        assert_eq!(symbol_key("src/a.ts", "foo"), "src/a.ts#foo");
        assert_eq!(module_symbol_key("src/a.ts"), "src/a.ts#__module__");
    }
}
