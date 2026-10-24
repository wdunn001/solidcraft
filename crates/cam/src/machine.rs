//! OEM and hobby machine presets, plus a session-owned machine plugin host.
//!
//! Profiles are planning metadata (ATC, probe, integration port). Live I/O lives behind
//! [`MachineBackend`]. Tests use [`FakeMachineHost`]; the shop floor uses MTConnect (fixture
//! XML or loopback HTTP). Fanuc FOCAS and Okuma THINC register as unavailable stubs until
//! their SDKs are linked.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Whether a plugin currently answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineAxes {
    ThreeAxis,
    FiveAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpindleTaper {
    ErCollet,
    Cat40,
    Bt40,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChangerKind {
    None,
    Manual,
    SideMount,
    Arm,
    Carousel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolChanger {
    pub kind: ToolChangerKind,
    pub pockets: u32,
    #[serde(default)]
    pub random_access: bool,
    #[serde(default)]
    pub dual_arm: bool,
    #[serde(default)]
    pub change_time_s: f64,
    #[serde(default)]
    pub max_tool_diameter_mm: f64,
    #[serde(default)]
    pub max_tool_length_mm: f64,
    #[serde(default)]
    pub max_tool_weight_kg: f64,
}

impl Default for ToolChanger {
    fn default() -> Self {
        Self {
            kind: ToolChangerKind::Manual,
            pockets: 1,
            random_access: false,
            dual_arm: false,
            change_time_s: 0.0,
            max_tool_diameter_mm: 0.0,
            max_tool_length_mm: 0.0,
            max_tool_weight_kg: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineIntegrationKind {
    SerialGcode,
    FileGcode,
    JczLaser,
    MtConnect,
    Thinc,
    Focas,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineIntegration {
    pub kind: MachineIntegrationKind,
    #[serde(default)]
    pub default_port: u16,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeMacroStyle {
    None,
    HaasInspectionPlus,
    FanucInspectionPlus,
}

impl Default for ProbeMacroStyle {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeCapability {
    #[serde(default)]
    pub present: bool,
    #[serde(default)]
    pub style: ProbeMacroStyle,
    #[serde(default)]
    pub tool_number: u32,
    #[serde(default)]
    pub notes: String,
}

impl Default for ProbeCapability {
    fn default() -> Self {
        Self { present: false, style: ProbeMacroStyle::None, tool_number: 0, notes: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineProfile {
    pub name: String,
    pub oem: String,
    pub model: String,
    pub axes: MachineAxes,
    pub spindle_taper: SpindleTaper,
    pub tool_changer: ToolChanger,
    pub integration: MachineIntegration,
    pub probe: ProbeCapability,
    pub max_spindle_rpm: u32,
    pub max_feedrate_mmpm: f64,
    pub rapid_feedrate_mmpm: f64,
}

fn manual_er(name: &str, oem: &str, model: &str, rpm: u32, feed: f64, rapid: f64) -> MachineProfile {
    MachineProfile {
        name: name.into(),
        oem: oem.into(),
        model: model.into(),
        axes: MachineAxes::ThreeAxis,
        spindle_taper: SpindleTaper::ErCollet,
        tool_changer: ToolChanger { kind: ToolChangerKind::Manual, pockets: 1, ..ToolChanger::default() },
        integration: MachineIntegration { kind: MachineIntegrationKind::FileGcode, default_port: 0, notes: String::new(), source: String::new() },
        probe: ProbeCapability::default(),
        max_spindle_rpm: rpm,
        max_feedrate_mmpm: feed,
        rapid_feedrate_mmpm: rapid,
    }
}

/// Built-in machine presets. Hobby / file-or-serial first, then shop OEM with cited ports.
pub fn preset_machines() -> Vec<(&'static str, MachineProfile)> {
    let mut out = Vec::new();
    {
        let mut m = manual_er("GRBL Router", "GRBL", "Generic GRBL / FluidNC", 24_000, 3_000.0, 5_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::SerialGcode,
            default_port: 0,
            notes: "Post GRBL G-code to a file, then send with a GRBL sender. No MTConnect.".into(),
            source: "GRBL / grblHAL / FluidNC public docs; SolidCraft GRBL post".into(),
        };
        out.push(("GRBL Router", m));
    }
    for (id, model, notes) in [
        ("ChinaCNC 3040", "3040 (Mach3)", "Typical 300x400 China bed on Mach3. Post Mach3 NC file."),
        ("ChinaCNC 6040", "6040 (Mach3)", "400x600 Mach3-class bed. File post only."),
        ("ChinaCNC 6090", "6090 (Mach3)", "600x900 Mach3-class bed. Post NC file to the control PC."),
    ] {
        let mut m = manual_er(id, "ChinaCNCzone", model, 24_000, 2_000.0, 3_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::FileGcode,
            default_port: 0,
            notes: notes.into(),
            source: "china-cncrouter / ChinaCNCzone bed sizes; SolidCraft Mach3 post".into(),
        };
        out.push((id, m));
    }
    {
        let mut m = manual_er("Makera Carvera", "Makera", "Carvera", 15_000, 2_000.0, 3_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::FileGcode,
            default_port: 0,
            notes: "Desktop CNC (360x240). Post GRBL-class G-code; run via Makera control.".into(),
            source: "Makera Carvera public specs; SolidCraft GRBL post".into(),
        };
        out.push(("Makera Carvera", m));
    }
    {
        let mut m = manual_er("Makera Carvera Air", "Makera", "Carvera Air", 15_000, 2_000.0, 3_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::FileGcode,
            default_port: 0,
            notes: "Air variant (300x200). File G-code handoff; not MTConnect.".into(),
            source: "Makera Carvera Air public specs; SolidCraft GRBL post".into(),
        };
        out.push(("Makera Carvera Air", m));
    }
    {
        let mut m = manual_er("Snapmaker Artisan", "Snapmaker", "Artisan (Marlin)", 12_000, 3_000.0, 3_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::SerialGcode,
            default_port: 0,
            notes: "Marlin firmware (not GRBL). Laser work area 400x400. Use vendor app or serial.".into(),
            source: "Snapmaker Artisan docs; SolidCraft posts; LightBurn Snapmaker device".into(),
        };
        out.push(("Snapmaker Artisan", m));
    }
    {
        let mut m = manual_er("Snapmaker A350", "Snapmaker", "2.0 A350 (Marlin)", 12_000, 3_000.0, 3_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::SerialGcode,
            default_port: 0,
            notes: "Snapmaker 2.0 Marlin. Laser bed 320x350.".into(),
            source: "Snapmaker 2.0 docs".into(),
        };
        out.push(("Snapmaker A350", m));
    }
    {
        let mut m = manual_er("Repetier / Marlin sender", "Repetier", "Repetier-Host / Firmware", 24_000, 3_000.0, 5_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::SerialGcode,
            default_port: 0,
            notes: "Post Marlin G-code to a file, then open in Repetier-Host.".into(),
            source: "Repetier-Host / Repetier-Firmware public docs".into(),
        };
        out.push(("Repetier", m));
    }
    out.push((
        "BJJCZ JCZ",
        MachineProfile {
            name: "BJJCZ / JCZ Fiber".into(),
            oem: "BJJCZ".into(),
            model: "JCZ / EzCAD galvo".into(),
            axes: MachineAxes::ThreeAxis,
            spindle_taper: SpindleTaper::Unknown,
            tool_changer: ToolChanger { kind: ToolChangerKind::None, pockets: 0, ..ToolChanger::default() },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::JczLaser,
                default_port: 0,
                notes: "Galvo fiber (ComMarker-class). Mill posts do not drive the card; use laser.exportPackage.".into(),
                source: "BJJCZ/JCZ + LightBurn JCZFiber; SolidCraft laser handoff".into(),
            },
            probe: ProbeCapability::default(),
            max_spindle_rpm: 0,
            max_feedrate_mmpm: 0.0,
            rapid_feedrate_mmpm: 0.0,
        },
    ));
    {
        let mut m = manual_er("LinuxCNC Desktop", "LinuxCNC", "Generic", 24_000, 5_000.0, 8_000.0);
        m.integration = MachineIntegration {
            kind: MachineIntegrationKind::FileGcode,
            default_port: 0,
            notes: "Post LinuxCNC G-code; run under AXIS / Gmoccapy.".into(),
            source: "LinuxCNC docs; SolidCraft RS-274 post".into(),
        };
        out.push(("LinuxCNC", m));
    }
    out.push((
        "Haas VF-2",
        MachineProfile {
            name: "Haas VF-2".into(),
            oem: "Haas".into(),
            model: "VF-2 NGC".into(),
            axes: MachineAxes::ThreeAxis,
            spindle_taper: SpindleTaper::Cat40,
            tool_changer: ToolChanger {
                kind: ToolChangerKind::SideMount,
                pockets: 20,
                random_access: true,
                dual_arm: false,
                change_time_s: 4.2,
                max_tool_diameter_mm: 76.0,
                max_tool_length_mm: 279.0,
                max_tool_weight_kg: 5.4,
            },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::MtConnect,
                default_port: 8082,
                notes: "Haas NGC MTConnect agent over HTTP at http://<ip>:8082/<MODEL>/current.".into(),
                source: "Haas NGC Machine Data Collection (MTConnect port 8082)".into(),
            },
            probe: ProbeCapability {
                present: true,
                style: ProbeMacroStyle::HaasInspectionPlus,
                tool_number: 25,
                notes: "Renishaw Inspection Plus on Haas (G65 P981x; P9832 on / P9833 off). Confirm pocket and calib.".into(),
            },
            max_spindle_rpm: 8_100,
            max_feedrate_mmpm: 25_400.0,
            rapid_feedrate_mmpm: 25_400.0,
        },
    ));
    out.push((
        "Haas UMC-750",
        MachineProfile {
            name: "Haas UMC-750".into(),
            oem: "Haas".into(),
            model: "UMC-750".into(),
            axes: MachineAxes::FiveAxis,
            spindle_taper: SpindleTaper::Cat40,
            tool_changer: ToolChanger {
                kind: ToolChangerKind::SideMount,
                pockets: 40,
                random_access: true,
                dual_arm: false,
                change_time_s: 4.5,
                max_tool_diameter_mm: 76.0,
                max_tool_length_mm: 279.0,
                max_tool_weight_kg: 5.4,
            },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::MtConnect,
                default_port: 8082,
                notes: "Same NGC MTConnect surface as other Haas mills.".into(),
                source: "Haas NGC Machine Data Collection (MTConnect port 8082)".into(),
            },
            probe: ProbeCapability {
                present: true,
                style: ProbeMacroStyle::HaasInspectionPlus,
                tool_number: 40,
                notes: "UMC typically carries a spindle probe in a high ATC pocket.".into(),
            },
            max_spindle_rpm: 8_100,
            max_feedrate_mmpm: 22_900.0,
            rapid_feedrate_mmpm: 22_900.0,
        },
    ));
    out.push((
        "Okuma GENOS M560",
        MachineProfile {
            name: "Okuma GENOS M560".into(),
            oem: "Okuma".into(),
            model: "GENOS M560-V".into(),
            axes: MachineAxes::ThreeAxis,
            spindle_taper: SpindleTaper::Cat40,
            tool_changer: ToolChanger {
                kind: ToolChangerKind::Arm,
                pockets: 32,
                random_access: true,
                dual_arm: true,
                change_time_s: 1.3,
                max_tool_diameter_mm: 80.0,
                max_tool_length_mm: 300.0,
                max_tool_weight_kg: 8.0,
            },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::Thinc,
                default_port: 5000,
                notes: "Prefer Okuma THINC Open API; MTConnect agent often on :5000. THINC plugin is planned.".into(),
                source: "Okuma Open API SDK (THINC); Okuma MTConnect Agent and Adapter".into(),
            },
            probe: ProbeCapability::default(),
            max_spindle_rpm: 15_000,
            max_feedrate_mmpm: 40_000.0,
            rapid_feedrate_mmpm: 40_000.0,
        },
    ));
    out.push((
        "Fanuc 0i-MF",
        MachineProfile {
            name: "Fanuc 0i-MF class".into(),
            oem: "Fanuc".into(),
            model: "0i-MF".into(),
            axes: MachineAxes::ThreeAxis,
            spindle_taper: SpindleTaper::Bt40,
            tool_changer: ToolChanger {
                kind: ToolChangerKind::Carousel,
                pockets: 24,
                random_access: true,
                dual_arm: false,
                change_time_s: 2.5,
                max_tool_diameter_mm: 80.0,
                max_tool_length_mm: 300.0,
                max_tool_weight_kg: 7.0,
            },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::Focas,
                default_port: 8193,
                notes: "Fanuc FOCAS Ethernet (covers most DN Solutions Fanuc-control mills). FOCAS plugin is planned.".into(),
                source: "FANUC FOCAS / FASMTC MTConnect Server (FOCAS to MTConnect)".into(),
            },
            probe: ProbeCapability {
                present: true,
                style: ProbeMacroStyle::FanucInspectionPlus,
                tool_number: 24,
                notes: "Inspection Plus O98xx when Renishaw macros are installed.".into(),
            },
            max_spindle_rpm: 12_000,
            max_feedrate_mmpm: 30_000.0,
            rapid_feedrate_mmpm: 30_000.0,
        },
    ));
    out.push((
        "Mazak VCN-530C",
        MachineProfile {
            name: "Mazak VCN-530C".into(),
            oem: "Mazak".into(),
            model: "VCN-530C Smooth".into(),
            axes: MachineAxes::ThreeAxis,
            spindle_taper: SpindleTaper::Cat40,
            tool_changer: ToolChanger {
                kind: ToolChangerKind::Carousel,
                pockets: 30,
                random_access: true,
                dual_arm: false,
                change_time_s: 2.0,
                max_tool_diameter_mm: 80.0,
                max_tool_length_mm: 300.0,
                max_tool_weight_kg: 8.0,
            },
            integration: MachineIntegration {
                kind: MachineIntegrationKind::MtConnect,
                default_port: 5000,
                notes: "Mazak Smooth native MTConnect (SmoothX/G/C). Read-oriented.".into(),
                source: "Mazak Smooth CNC MTConnect functionality (MTConnect 1.3)".into(),
            },
            probe: ProbeCapability::default(),
            max_spindle_rpm: 12_000,
            max_feedrate_mmpm: 42_000.0,
            rapid_feedrate_mmpm: 42_000.0,
        },
    ));
    out
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineBackendDescriptor {
    pub id: String,
    pub label: String,
    pub protocol: String,
    pub availability: Availability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MachineLiveStatus {
    pub available: bool,
    pub execution: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_rpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedrate_mmpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub source: String,
}

impl Default for MachineLiveStatus {
    fn default() -> Self {
        Self {
            available: false,
            execution: "UNAVAILABLE".into(),
            program: None,
            tool_number: None,
            spindle_rpm: None,
            feedrate_mmpm: None,
            message: None,
            source: "none".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct MachineError {
    pub message: String,
}

impl MachineError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

pub type MachineResult<T> = Result<T, MachineError>;

pub trait MachineBackend: Send + Sync + 'static {
    fn descriptor(&self) -> MachineBackendDescriptor;
    fn is_available(&self) -> bool;
    fn status(&self) -> MachineResult<MachineLiveStatus>;
}

#[derive(Clone)]
pub struct MachineHost(Arc<dyn MachineBackend>);

impl MachineHost {
    pub fn new(backend: impl MachineBackend) -> Self {
        MachineHost(Arc::new(backend))
    }

    pub fn fake() -> Self {
        MachineHost::new(FakeMachineHost::default())
    }

    pub fn descriptor(&self) -> MachineBackendDescriptor {
        self.0.descriptor()
    }

    pub fn is_available(&self) -> bool {
        self.0.is_available()
    }

    pub fn status(&self) -> MachineResult<MachineLiveStatus> {
        self.0.status()
    }
}

#[derive(Clone, Debug)]
pub struct FakeMachineHost {
    pub status: MachineLiveStatus,
}

impl Default for FakeMachineHost {
    fn default() -> Self {
        Self {
            status: MachineLiveStatus {
                available: true,
                execution: "READY".into(),
                program: Some("O1000".into()),
                tool_number: Some(1),
                spindle_rpm: Some(0.0),
                feedrate_mmpm: Some(0.0),
                message: Some("fake machine plugin".into()),
                source: "fake".into(),
            },
        }
    }
}

impl MachineBackend for FakeMachineHost {
    fn descriptor(&self) -> MachineBackendDescriptor {
        MachineBackendDescriptor {
            id: "fake-machine".into(),
            label: "Fake machine (tests)".into(),
            protocol: "fake".into(),
            availability: Availability::Available,
            note: Some("Deterministic status for plugin wiring tests".into()),
            endpoint: None,
        }
    }

    fn is_available(&self) -> bool {
        self.status.available
    }

    fn status(&self) -> MachineResult<MachineLiveStatus> {
        Ok(self.status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_include_hobby_and_oem() {
        let presets = preset_machines();
        assert!(presets.len() >= 15);
        for (id, m) in &presets {
            assert!(!id.is_empty());
            assert!(!m.name.is_empty());
            assert!(!m.integration.source.is_empty());
        }
        assert!(presets.iter().any(|(id, _)| *id == "GRBL Router"));
        assert!(presets.iter().any(|(id, _)| *id == "Haas VF-2"));
        assert!(presets.iter().any(|(id, _)| *id == "Fanuc 0i-MF"));
        assert!(presets.iter().any(|(_, m)| m.probe.style == ProbeMacroStyle::HaasInspectionPlus));
        assert!(presets.iter().any(|(_, m)| m.integration.kind == MachineIntegrationKind::Focas));
    }

    #[test]
    fn fake_host_reports_ready() {
        let host = MachineHost::fake();
        assert!(host.is_available());
        let st = host.status().unwrap();
        assert_eq!(st.execution, "READY");
        assert_eq!(host.descriptor().protocol, "fake");
    }
}
