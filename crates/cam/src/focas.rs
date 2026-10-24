//! Fanuc FOCAS machine adapter stub.
//!
//! Registers the protocol so OEM profiles can name FOCAS (Ethernet port 8193). The Fanuc
//! FOCAS SDK is not linked; live CNC I/O is not available in this build.

use super::machine::{Availability, MachineBackend, MachineBackendDescriptor, MachineLiveStatus, MachineResult};

/// Placeholder Fanuc FOCAS backend (SDK not linked).
#[derive(Clone, Debug, Default)]
pub struct FocasBackend {
    pub endpoint: Option<String>,
}

impl FocasBackend {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        Self { endpoint: if endpoint.trim().is_empty() { None } else { Some(endpoint) } }
    }
}

impl MachineBackend for FocasBackend {
    fn descriptor(&self) -> MachineBackendDescriptor {
        MachineBackendDescriptor {
            id: "focas".into(),
            label: "Fanuc FOCAS (stub)".into(),
            protocol: "focas".into(),
            availability: Availability::Unavailable,
            note: Some("FOCAS SDK not linked in this build. Source: FANUC FOCAS / FASMTC MTConnect Server.".into()),
            endpoint: self.endpoint.clone(),
        }
    }

    fn is_available(&self) -> bool {
        false
    }

    fn status(&self) -> MachineResult<MachineLiveStatus> {
        Ok(MachineLiveStatus {
            available: false,
            execution: "UNAVAILABLE".into(),
            message: Some("FOCAS stub: SDK not linked".into()),
            source: "focas".into(),
            ..MachineLiveStatus::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MachineHost;

    #[test]
    fn stub_names_focas_and_stays_unavailable() {
        let host = MachineHost::new(FocasBackend::new("127.0.0.1:8193"));
        assert!(!host.is_available());
        let d = host.descriptor();
        assert_eq!(d.id, "focas");
        assert_eq!(d.protocol, "focas");
        assert_eq!(d.availability, Availability::Unavailable);
        assert_eq!(d.endpoint.as_deref(), Some("127.0.0.1:8193"));
        let st = host.status().unwrap();
        assert!(!st.available);
        assert_eq!(st.source, "focas");
        assert_eq!(st.execution, "UNAVAILABLE");
    }
}
