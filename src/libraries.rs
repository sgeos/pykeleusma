//! Keleusma's bundled native libraries, described in the book's chapter 33.
//!
//! `Math` and `Audio` are pure computations. `Shell` reaches the host's environment,
//! processes and files, so a script can use it only when its host names it in
//! `libraries=`, and the host bears the consequences of offering it.

use keleusma::stddsl::{Audio, Math, Shell};

/// A bundled library, by the name Python uses for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    Math,
    Audio,
    Shell,
}

impl Library {
    /// The library called `name`, or `None` for an unknown name.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "math" => Some(Self::Math),
            "audio" => Some(Self::Audio),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }

    /// The `use` declarations Keleusma publishes for the library, which a script may
    /// include to have its calls type-checked.
    pub fn signatures(self) -> &'static str {
        match self {
            Self::Math => Math::SIGNATURES,
            Self::Audio => Audio::SIGNATURES,
            Self::Shell => Shell::SIGNATURES,
        }
    }

    /// The full names of the natives the library registers, read from its signatures.
    pub fn native_names(self) -> Vec<String> {
        self.signatures()
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("use ")?;
                let end = rest.find(['(', ' ']).unwrap_or(rest.len());
                Some(rest[..end].to_string())
            })
            .collect()
    }

    /// Register the library's natives on `vm`.
    pub fn register(self, vm: &mut keleusma::vm::Vm<'static, '_>) {
        match self {
            Self::Math => vm.register_library(Math),
            Self::Audio => vm.register_library(Audio),
            Self::Shell => vm.register_library(Shell),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_read_from_the_signatures() {
        let math = Library::Math.native_names();
        assert!(math.contains(&"math::sqrt".to_string()));
        assert!(math.contains(&"math::pi".to_string()));
        assert!(math.iter().all(|n| n.starts_with("math::")));
        assert!(
            Library::Audio
                .native_names()
                .contains(&"audio::midi_to_freq".to_string())
        );
        assert!(
            Library::Shell
                .native_names()
                .contains(&"shell::getenv".to_string())
        );
    }

    #[test]
    fn unknown_names_are_refused() {
        assert_eq!(Library::named("math"), Some(Library::Math));
        assert_eq!(Library::named("graphics"), None);
    }
}
