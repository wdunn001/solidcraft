//! Okuma THINC machine adapter stub.
//!
//! Registers the protocol so OEM profiles can name THINC (Open API, often :5000). The Okuma
//! THINC API is not linked; live CNC I/O is not available in this build.

use super::machine::{Availability, MachineBackend, MachineBackendDescriptor, MachineLiveStatus, MachineResult};

/// Placeholder Okuma THINC backend (API not linked).
#[derive(Clone, Debug, Default)]
pub struct ThincBackend {
    pub endpoint: Option<String>,
}

impl ThincBackend {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        Self { endpoint: if endpoint.trim().is_empty() { None } else { Some(endpoint) } }
    }
}

impl MachineBackend for ThincBackend {
    fn descriptor(&self) -> MachineBackendDescriptor {
        MachineBackendDescriptor {
            id: "thinc".into(),
            label: "Okuma THINC (stub)".into(),
            protocol: "thinc".into(),
            availability: Availability::Unavailable,
            note: Some("THINC API not linked in this build. Source: Okuma Open API SDK (THINC).".into()),
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
            message: Some("THINC stub: API not linked".into()),
            source: "thinc".into(),
            ..MachineLiveStatus::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MachineHost;

    #[test]
    fn stub_names_thinc_and_stays_unavailable() {
        let host = MachineHost::new(ThincBackend::new("127.0.0.1:5000"));
        assert!(!host.is_available());
        let d = host.descriptor();
        assert_eq!(d.id, "thinc");
        assert_eq!(d.protocol, "thinc");
        assert_eq!(d.availability, Availability::Unavailable);
        assert_eq!(d.endpoint.as_deref(), Some("127.0.0.1:5000"));
        let st = host.status().unwrap();
        assert!(!st.available);
        assert_eq!(st.source, "thinc");
        assert_eq!(st.execution, "UNAVAILABLE");
    }
}
