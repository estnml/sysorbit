//! What the host's systemd can do.
//!
//! Version thresholds come from systemd's NEWS file. Each feature has one
//! fallback, so supporting an older systemd never means a code path per
//! version. Callers still fall back when a command fails, because distros
//! backport features and some builds disable them.

use zbus::Connection;

use super::dbus::ManagerProxy;
use crate::model::Warning;

/// Oldest systemd the app is built for (RHEL 8).
pub const MINIMUM_VERSION: u32 = 239;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Major version, `None` when systemd reported something unparsable.
    pub version: Option<u32>,
}

impl Capabilities {
    pub async fn detect(bus: &Connection) -> Self {
        let version = match ManagerProxy::new(bus).await {
            Ok(manager) => manager
                .version()
                .await
                .inspect_err(|error| {
                    crate::diagnostics::error(crate::diagnostics::bus_error(error));
                })
                .ok(),
            Err(error) => {
                crate::diagnostics::error(crate::diagnostics::bus_error(&error));
                None
            }
        };
        let capabilities = Self::from_version(version.as_deref().unwrap_or_default());
        if capabilities.version.is_none() && version.is_some() {
            crate::diagnostics::parse_error(crate::diagnostics::Parser::Capabilities);
        }
        capabilities
    }

    /// Parses `Manager.Version`, e.g. `262.1-1-arch`, `249.11-0ubuntu3.12` or
    /// `239 (239-82.el8)`.
    pub fn from_version(version: &str) -> Self {
        let digits: String = version
            .trim()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        Self {
            version: digits.parse().ok(),
        }
    }

    /// `systemd-analyze calendar --iterations=`, added in 242.
    pub fn calendar_iterations(self) -> bool {
        self.at_least(242)
    }

    /// `systemd-analyze calendar --base-time=`, added in 244.
    pub fn calendar_base_time(self) -> bool {
        self.at_least(244)
    }

    /// `journalctl --list-boots --output=json`, added in 251.
    pub fn boots_json(self) -> bool {
        self.at_least(251)
    }

    /// What this systemd cannot show.
    pub fn limitations(self) -> Vec<Warning> {
        let mut notes = Vec::new();
        if let Some(version) = self.version.filter(|&v| v < MINIMUM_VERSION) {
            notes.push(Warning::OldSystemd {
                version,
                minimum: MINIMUM_VERSION,
            });
        }
        if !self.calendar_iterations() {
            notes.push(Warning::NextElapseOnly);
        } else if !self.calendar_base_time() {
            notes.push(Warning::NoPastElapses);
        }
        notes
    }

    /// Unknown versions are assumed modern; failing commands still fall back.
    fn at_least(self, version: u32) -> bool {
        self.version.is_none_or(|have| have >= version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_distro_version_strings() {
        assert_eq!(
            Capabilities::from_version("262.1-1-arch").version,
            Some(262)
        );
        assert_eq!(
            Capabilities::from_version("249.11-0ubuntu3.12").version,
            Some(249)
        );
        assert_eq!(
            Capabilities::from_version("239 (239-82.el8)").version,
            Some(239)
        );
        assert_eq!(Capabilities::from_version("").version, None);
    }

    #[test]
    fn gates_features_by_version() {
        let rhel8 = Capabilities::from_version("239");
        assert!(!rhel8.calendar_iterations() && !rhel8.boots_json());

        let debian11 = Capabilities::from_version("247.3-7");
        assert!(debian11.calendar_base_time() && !debian11.boots_json());

        let unknown = Capabilities::from_version("");
        assert!(unknown.boots_json() && unknown.limitations().is_empty());
    }
}
