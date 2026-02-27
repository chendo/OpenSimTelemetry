//! Cross-platform .ibt file parser for iRacing telemetry replay
//!
//! Parses iRacing binary telemetry (.ibt) files and converts samples
//! to TelemetryFrame for replay. Works on all platforms.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use ost_core::{model::*, units::*};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

// ============================================================================
// Binary format types
// ============================================================================

/// Variable data types in .ibt files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Char = 0,
    Bool = 1,
    Int = 2,
    BitField = 3,
    Float = 4,
    Double = 5,
}

impl VarType {
    fn from_i32(val: i32) -> Result<Self> {
        match val {
            0 => Ok(VarType::Char),
            1 => Ok(VarType::Bool),
            2 => Ok(VarType::Int),
            3 => Ok(VarType::BitField),
            4 => Ok(VarType::Float),
            5 => Ok(VarType::Double),
            _ => bail!("Unknown variable type: {}", val),
        }
    }

    /// Size in bytes for a single element of this type
    fn element_size(&self) -> usize {
        match self {
            VarType::Char => 1,
            VarType::Bool => 1,
            VarType::Int => 4,
            VarType::BitField => 4,
            VarType::Float => 4,
            VarType::Double => 8,
        }
    }
}

/// A parsed variable value from a sample
#[derive(Debug, Clone)]
pub enum VarValue {
    Char(u8),
    Bool(bool),
    Int(i32),
    BitField(u32),
    Float(f32),
    Double(f64),
    CharArray(Vec<u8>),
    IntArray(Vec<i32>),
    FloatArray(Vec<f32>),
    DoubleArray(Vec<f64>),
}

impl VarValue {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            VarValue::Float(v) => Some(*v),
            VarValue::Double(v) => Some(*v as f32),
            VarValue::Int(v) => Some(*v as f32),
            VarValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            VarValue::Double(v) => Some(*v),
            VarValue::Float(v) => Some(*v as f64),
            VarValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            VarValue::Int(v) => Some(*v),
            VarValue::BitField(v) => Some(*v as i32),
            VarValue::Float(v) => Some(*v as i32),
            VarValue::Bool(v) => Some(if *v { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            VarValue::Bool(v) => Some(*v),
            VarValue::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            VarValue::BitField(v) => Some(*v),
            VarValue::Int(v) => Some(*v as u32),
            _ => None,
        }
    }
}

/// Main .ibt file header (48 bytes at offset 0)
#[derive(Debug, Clone)]
pub struct IbtHeader {
    pub ver: i32,
    pub status: i32,
    pub tick_rate: i32,
    pub session_info_update: i32,
    pub session_info_len: i32,
    pub session_info_offset: i32,
    pub num_vars: i32,
    pub var_header_offset: i32,
    pub num_buf: i32,
    pub buf_len: i32,
}

/// Variable buffer descriptor (one of 4, 16 bytes each)
#[derive(Debug, Clone)]
pub struct VarBuf {
    pub tick_count: i32,
    pub buf_offset: i32,
}

/// Disk sub-header (32 bytes at offset 112)
#[derive(Debug, Clone)]
pub struct DiskSubHeader {
    pub session_start_date: i64,
    pub session_start_time: f64,
    pub session_end_time: f64,
    pub session_lap_count: i32,
    pub session_record_count: i32,
}

/// A single variable header (144 bytes each)
#[derive(Debug, Clone)]
pub struct VarHeader {
    pub var_type: VarType,
    pub offset: i32,
    pub count: i32,
    pub count_as_time: bool,
    pub name: String,
    pub desc: String,
    pub unit: String,
}

// ============================================================================
// Session info parsed from YAML
// ============================================================================

/// Key session info extracted from the YAML string in the .ibt file
#[derive(Debug, Clone, Default)]
pub struct IbtSessionInfo {
    pub track_name: String,
    pub track_display_name: String,
    pub track_config_name: String,
    pub track_length: String,
    pub car_name: String,
    pub car_screen_name: String,
    pub driver_name: String,
    pub driver_car_idx: i32,
    pub session_type: String,
}

impl IbtSessionInfo {
    /// Parse session info from the YAML string.
    /// Uses simple line-based parsing to avoid adding a YAML dependency.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let mut info = IbtSessionInfo::default();

        for line in yaml.lines() {
            let trimmed = line.trim();

            if let Some(val) = try_extract_yaml_value(trimmed, "TrackName:") {
                info.track_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackDisplayName:") {
                info.track_display_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackConfigName:") {
                info.track_config_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackLength:") {
                info.track_length = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "CarScreenName:") {
                if info.car_screen_name.is_empty() {
                    info.car_screen_name = val;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "UserName:") {
                if info.driver_name.is_empty() {
                    info.driver_name = val;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "DriverCarIdx:") {
                if let Ok(idx) = val.parse::<i32>() {
                    info.driver_car_idx = idx;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "SessionType:") {
                if info.session_type.is_empty() {
                    info.session_type = val;
                }
            }
        }

        if info.track_display_name.is_empty() {
            info.track_display_name = info.track_name.clone();
        }

        info.car_name = info.car_screen_name.clone();

        Ok(info)
    }
}

fn try_extract_yaml_value(line: &str, key: &str) -> Option<String> {
    line.strip_prefix(key).map(|rest| rest.trim().to_string())
}

// ============================================================================
// IbtFile: main parser
// ============================================================================

/// Parsed .ibt file handle for reading telemetry samples
pub struct IbtFile {
    file: File,
    pub header: IbtHeader,
    pub disk_sub_header: DiskSubHeader,
    pub var_headers: Vec<VarHeader>,
    pub session_info_yaml: String,
    pub session_info: IbtSessionInfo,
    sample_data_offset: u64,
    file_size: u64,
    #[allow(dead_code)]
    var_index: HashMap<String, usize>,
}

impl IbtFile {
    /// Open and parse an .ibt file from disk.
    /// Reads headers and session info, but does NOT load sample data into memory.
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open .ibt file: {}", path.display()))?;

        let file_size = file.metadata()?.len();

        let header = Self::read_header(&mut file)?;

        file.seek(SeekFrom::Start(48))?;
        let var_buf = Self::read_var_buf(&mut file)?;

        file.seek(SeekFrom::Start(112))?;
        let disk_sub_header = Self::read_disk_sub_header(&mut file)?;

        file.seek(SeekFrom::Start(header.var_header_offset as u64))?;
        let var_headers = Self::read_var_headers(&mut file, header.num_vars as usize)?;

        let var_index: HashMap<String, usize> = var_headers
            .iter()
            .enumerate()
            .map(|(i, vh)| (vh.name.clone(), i))
            .collect();

        file.seek(SeekFrom::Start(header.session_info_offset as u64))?;
        let mut yaml_buf = vec![0u8; header.session_info_len as usize];
        file.read_exact(&mut yaml_buf)?;
        let yaml_end = yaml_buf
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(yaml_buf.len());
        let session_info_yaml = String::from_utf8_lossy(&yaml_buf[..yaml_end]).to_string();

        let session_info = IbtSessionInfo::from_yaml(&session_info_yaml).unwrap_or_default();

        let sample_data_offset = var_buf.buf_offset as u64;

        Ok(IbtFile {
            file,
            header,
            disk_sub_header,
            var_headers,
            session_info_yaml,
            session_info,
            sample_data_offset,
            file_size,
            var_index,
        })
    }

    fn read_header(file: &mut File) -> Result<IbtHeader> {
        file.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; 48];
        file.read_exact(&mut buf)?;

        Ok(IbtHeader {
            ver: i32::from_le_bytes(buf[0..4].try_into()?),
            status: i32::from_le_bytes(buf[4..8].try_into()?),
            tick_rate: i32::from_le_bytes(buf[8..12].try_into()?),
            session_info_update: i32::from_le_bytes(buf[12..16].try_into()?),
            session_info_len: i32::from_le_bytes(buf[16..20].try_into()?),
            session_info_offset: i32::from_le_bytes(buf[20..24].try_into()?),
            num_vars: i32::from_le_bytes(buf[24..28].try_into()?),
            var_header_offset: i32::from_le_bytes(buf[28..32].try_into()?),
            num_buf: i32::from_le_bytes(buf[32..36].try_into()?),
            buf_len: i32::from_le_bytes(buf[36..40].try_into()?),
        })
    }

    fn read_var_buf(file: &mut File) -> Result<VarBuf> {
        let mut buf = [0u8; 16];
        file.read_exact(&mut buf)?;

        Ok(VarBuf {
            tick_count: i32::from_le_bytes(buf[0..4].try_into()?),
            buf_offset: i32::from_le_bytes(buf[4..8].try_into()?),
        })
    }

    fn read_disk_sub_header(file: &mut File) -> Result<DiskSubHeader> {
        let mut buf = [0u8; 32];
        file.read_exact(&mut buf)?;

        Ok(DiskSubHeader {
            session_start_date: i64::from_le_bytes(buf[0..8].try_into()?),
            session_start_time: f64::from_le_bytes(buf[8..16].try_into()?),
            session_end_time: f64::from_le_bytes(buf[16..24].try_into()?),
            session_lap_count: i32::from_le_bytes(buf[24..28].try_into()?),
            session_record_count: i32::from_le_bytes(buf[28..32].try_into()?),
        })
    }

    fn read_var_headers(file: &mut File, count: usize) -> Result<Vec<VarHeader>> {
        let mut headers = Vec::with_capacity(count);

        for i in 0..count {
            let mut buf = [0u8; 144];
            file.read_exact(&mut buf)
                .with_context(|| format!("Failed to read variable header {}", i))?;

            let var_type = VarType::from_i32(i32::from_le_bytes(buf[0..4].try_into()?))?;
            let offset = i32::from_le_bytes(buf[4..8].try_into()?);
            let count = i32::from_le_bytes(buf[8..12].try_into()?);
            let count_as_time = buf[12] != 0;

            let name = read_null_terminated_string(&buf[16..48]);
            let desc = read_null_terminated_string(&buf[48..112]);
            let unit = read_null_terminated_string(&buf[112..144]);

            headers.push(VarHeader {
                var_type,
                offset,
                count,
                count_as_time,
                name,
                desc,
                unit,
            });
        }

        Ok(headers)
    }

    pub fn record_count(&self) -> usize {
        self.disk_sub_header.session_record_count as usize
    }

    pub fn tick_rate(&self) -> u32 {
        self.header.tick_rate as u32
    }

    pub fn duration_secs(&self) -> f64 {
        self.disk_sub_header.session_end_time - self.disk_sub_header.session_start_time
    }

    pub fn session_info_yaml(&self) -> &str {
        &self.session_info_yaml
    }

    pub fn session_info(&self) -> &IbtSessionInfo {
        &self.session_info
    }

    pub fn var_headers_ref(&self) -> &[VarHeader] {
        &self.var_headers
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Read a contiguous range of samples in a single disk operation.
    /// Much faster than calling `read_sample()` in a loop because it avoids
    /// per-frame seek overhead.
    pub fn read_samples_range(
        &mut self,
        start: usize,
        count: usize,
    ) -> Result<Vec<HashMap<String, VarValue>>> {
        let record_count = self.record_count();
        if start >= record_count {
            bail!(
                "Start index {} out of range (0..{})",
                start,
                record_count
            );
        }
        let clamped_count = count.min(record_count - start);
        if clamped_count == 0 {
            return Ok(Vec::new());
        }

        let buf_len = self.header.buf_len as usize;
        let offset = self.sample_data_offset + (start as u64) * (buf_len as u64);
        let total_bytes = buf_len * clamped_count;

        // Single seek + single read for the entire range
        self.file.seek(SeekFrom::Start(offset))?;
        let mut bulk_buf = vec![0u8; total_bytes];
        self.file.read_exact(&mut bulk_buf)?;

        // Parse each frame from the in-memory buffer
        let mut results = Vec::with_capacity(clamped_count);
        for i in 0..clamped_count {
            let frame_buf = &bulk_buf[i * buf_len..(i + 1) * buf_len];
            let mut sample = HashMap::with_capacity(self.var_headers.len());
            for vh in &self.var_headers {
                let var_offset = vh.offset as usize;
                let count = vh.count as usize;
                let end = var_offset + count * vh.var_type.element_size();
                if end > frame_buf.len() {
                    continue;
                }
                let value = if count == 1 {
                    read_scalar_value(frame_buf, var_offset, vh.var_type)
                } else {
                    read_array_value(frame_buf, var_offset, vh.var_type, count)
                };
                if let Some(val) = value {
                    sample.insert(vh.name.clone(), val);
                }
            }
            results.push(sample);
        }
        Ok(results)
    }

    /// Read a single sample by index, returning a HashMap of variable name -> VarValue
    pub fn read_sample(&mut self, index: usize) -> Result<HashMap<String, VarValue>> {
        let record_count = self.record_count();
        if index >= record_count {
            bail!(
                "Sample index {} out of range (0..{})",
                index,
                record_count
            );
        }

        let buf_len = self.header.buf_len as u64;
        let offset = self.sample_data_offset + (index as u64) * buf_len;

        self.file.seek(SeekFrom::Start(offset))?;
        let mut sample_buf = vec![0u8; buf_len as usize];
        self.file.read_exact(&mut sample_buf)?;

        let mut result = HashMap::with_capacity(self.var_headers.len());

        for vh in &self.var_headers {
            let var_offset = vh.offset as usize;
            let count = vh.count as usize;

            let end = var_offset + count * vh.var_type.element_size();
            if end > sample_buf.len() {
                continue;
            }

            let value = if count == 1 {
                read_scalar_value(&sample_buf, var_offset, vh.var_type)
            } else {
                read_array_value(&sample_buf, var_offset, vh.var_type, count)
            };

            if let Some(val) = value {
                result.insert(vh.name.clone(), val);
            }
        }

        Ok(result)
    }

    /// Variable names that are already mapped to structured TelemetryFrame fields.
    /// Everything NOT in this list (and not CarIdx*) goes into extras.
    /// Must stay in sync with MAPPED_VARS in iracing.rs.
    const MAPPED_VARS: &[&str] = &[
        // Motion
        "VelocityX", "VelocityY", "VelocityZ",
        "LatAccel", "LongAccel", "VertAccel",
        "Pitch", "Yaw", "Roll",
        "PitchRate", "YawRate", "RollRate",
        "Speed",
        // Vehicle
        "RPM", "Gear", "Throttle", "Brake", "Clutch",
        "SteeringWheelAngle", "SteeringWheelTorque", "SteeringWheelPctTorque",
        "IsOnTrack", "IsInGarage", "PlayerTrackSurface",
        // Engine
        "WaterTemp", "OilTemp", "OilPress", "OilLevel",
        "FuelLevel", "FuelLevelPct", "FuelPress", "FuelUsePerHour",
        "Voltage", "ManifoldPress", "EngineWarnings",
        // Wheels - LF
        "LFshockDefl", "LFshockDeflST", "LFshockVel", "LFshockVelST", "LFrideHeight",
        "LFairPressure", "LFcoldPressure",
        "LFtempCL", "LFtempCC", "LFtempCR",
        "LFtempL", "LFtempM", "LFtempR",
        "LFwear", "LFspeed", "LFbrakeLinePress",
        // Wheels - RF
        "RFshockDefl", "RFshockDeflST", "RFshockVel", "RFshockVelST", "RFrideHeight",
        "RFairPressure", "RFcoldPressure",
        "RFtempCL", "RFtempCC", "RFtempCR",
        "RFtempL", "RFtempM", "RFtempR",
        "RFwear", "RFspeed", "RFbrakeLinePress",
        // Wheels - LR
        "LRshockDefl", "LRshockDeflST", "LRshockVel", "LRshockVelST", "LRrideHeight",
        "LRairPressure", "LRcoldPressure",
        "LRtempCL", "LRtempCC", "LRtempCR",
        "LRtempL", "LRtempM", "LRtempR",
        "LRwear", "LRspeed", "LRbrakeLinePress",
        // Wheels - RR
        "RRshockDefl", "RRshockDeflST", "RRshockVel", "RRshockVelST", "RRrideHeight",
        "RRairPressure", "RRcoldPressure",
        "RRtempCL", "RRtempCC", "RRtempCR",
        "RRtempL", "RRtempM", "RRtempR",
        "RRwear", "RRspeed", "RRbrakeLinePress",
        // Timing
        "LapCurrentLapTime", "LapLastLapTime", "LapBestLapTime",
        "LapBestNLapTime", "LapBestNLapLap",
        "Lap", "LapCompleted", "LapDist", "LapDistPct",
        "PlayerCarPosition", "PlayerCarClassPosition",
        "LapDeltaToBestLap", "LapDeltaToBestLap_OK",
        "LapDeltaToSessionBestLap", "LapDeltaToSessionBestLap_OK",
        "LapDeltaToOptimalLap", "LapDeltaToOptimalLap_OK",
        "RaceLaps",
        // Session
        "SessionState", "SessionTime", "SessionTimeRemain", "SessionTimeOfDay",
        "SessionLapsRemainEx", "SessionFlags", "SessionNum",
        // Weather
        "AirTemp", "TrackTempCrew", "AirPressure", "AirDensity",
        "RelativeHumidity", "WindVel", "WindDir",
        "FogLevel", "Precipitation", "TrackWetness",
        "Skies", "WeatherDeclaredWet",
        // Pit
        "OnPitRoad", "PitstopActive", "PlayerCarPitSvStatus",
        "PitRepairLeft", "PitOptRepairLeft",
        "FastRepairAvailable", "FastRepairUsed",
        "dpFuelFill", "dpFuelAddKg",
        "dpLFTireChange", "dpRFTireChange", "dpLRTireChange", "dpRRTireChange",
        "dpLFTireColdPress", "dpRFTireColdPress", "dpLRTireColdPress", "dpRRTireColdPress",
        "dpWindshieldTearoff", "dpFastRepair",
        // Electronics
        "dcABS", "dcTractionControl", "dcTractionControl2",
        "dcBrakeBias", "dcAntiRollFront", "dcAntiRollRear",
        "DRS_Status", "dcThrottleShape",
        "PushToPass",
        // Per-car arrays
        "CarIdxLap", "CarIdxLapCompleted", "CarIdxLapDistPct",
        "CarIdxPosition", "CarIdxClassPosition",
        "CarIdxOnPitRoad", "CarIdxTrackSurface",
        "CarIdxBestLapTime", "CarIdxLastLapTime", "CarIdxEstTime",
        "CarIdxGear", "CarIdxRPM", "CarIdxSteer",
        // Tick
        "SessionTick",
    ];

    /// Convert a VarValue to a serde_json::Value for extras.
    fn var_value_to_json(value: &VarValue) -> serde_json::Value {
        match value {
            VarValue::Char(c) => serde_json::json!(*c),
            VarValue::Bool(b) => serde_json::json!(*b),
            VarValue::Int(i) => serde_json::json!(*i),
            VarValue::BitField(u) => serde_json::json!(*u),
            VarValue::Float(f) => serde_json::json!((*f * 10000.0).round() / 10000.0),
            VarValue::Double(d) => serde_json::json!((*d * 10000.0).round() / 10000.0),
            VarValue::CharArray(v) => {
                let s = String::from_utf8_lossy(v).trim_end_matches('\0').to_string();
                serde_json::json!(s)
            }
            VarValue::IntArray(v) => serde_json::json!(v),
            VarValue::FloatArray(v) => {
                let rounded: Vec<f32> = v.iter().map(|x| (x * 10000.0).round() / 10000.0).collect();
                serde_json::json!(rounded)
            }
            VarValue::DoubleArray(v) => {
                let rounded: Vec<f64> = v.iter().map(|x| (x * 10000.0).round() / 10000.0).collect();
                serde_json::json!(rounded)
            }
        }
    }

    /// Convert a raw sample HashMap to a TelemetryFrame.
    /// Mirrors the conversion logic from IRacingAdapter::convert_sample(),
    /// producing the nested sub-struct model.
    pub fn sample_to_frame(&self, sample: &HashMap<String, VarValue>) -> TelemetryFrame {
        let get_f32 = |name: &str| -> Option<f32> { sample.get(name).and_then(|v| v.as_f32()) };
        let get_f64 = |name: &str| -> Option<f64> { sample.get(name).and_then(|v| v.as_f64()) };
        let get_i32 = |name: &str| -> Option<i32> { sample.get(name).and_then(|v| v.as_i32()) };
        let get_u32 = |name: &str| -> Option<u32> { sample.get(name).and_then(|v| v.as_u32()) };
        let get_bool = |name: &str| -> Option<bool> { sample.get(name).and_then(|v| v.as_bool()) };

        let tick = get_i32("SessionTick").map(|t| t as u32);

        // =================================================================
        // Motion
        // =================================================================
        let velocity = match (
            get_f32("VelocityX"),
            get_f32("VelocityY"),
            get_f32("VelocityZ"),
        ) {
            (Some(vx), Some(vy), Some(vz)) => Some(Vector3::new(
                MetersPerSecond(vx),
                MetersPerSecond(vy),
                MetersPerSecond(vz),
            )),
            _ => None,
        };

        let acceleration = match (
            get_f32("LatAccel"),
            get_f32("LongAccel"),
            get_f32("VertAccel"),
        ) {
            (Some(lat), Some(long), Some(vert)) => Some(Vector3::new(
                MetersPerSecondSquared(lat),
                MetersPerSecondSquared(vert),
                MetersPerSecondSquared(long),
            )),
            _ => None,
        };

        let g_force = acceleration.as_ref().map(|a| {
            Vector3::new(
                GForce::from_acceleration(a.x),
                GForce::from_acceleration(a.y),
                GForce::from_acceleration(a.z),
            )
        });

        let rotation = match (get_f32("Pitch"), get_f32("Yaw"), get_f32("Roll")) {
            (Some(p), Some(y), Some(r)) => {
                Some(Vector3::new(Radians(p), Radians(y), Radians(r)))
            }
            _ => None,
        };

        let angular_velocity = match (
            get_f32("PitchRate"),
            get_f32("YawRate"),
            get_f32("RollRate"),
        ) {
            (Some(p), Some(y), Some(r)) => Some(Vector3::new(
                RadiansPerSecond(p),
                RadiansPerSecond(y),
                RadiansPerSecond(r),
            )),
            _ => None,
        };

        let motion = Some(MotionData {
            position: None,
            velocity,
            acceleration,
            g_force,
            rotation,
            angular_velocity,
            angular_acceleration: None,
        });

        // =================================================================
        // Vehicle
        // =================================================================
        let speed = get_f32("Speed").map(MetersPerSecond).or_else(|| {
            velocity.as_ref().map(|v| {
                MetersPerSecond((v.x.0.powi(2) + v.y.0.powi(2) + v.z.0.powi(2)).sqrt())
            })
        });

        let track_surface = get_i32("PlayerTrackSurface").map(|idx| match idx {
            -1 => TrackSurface::NotInWorld,
            0 => TrackSurface::Undefined,
            1..=4 => TrackSurface::Asphalt,
            6 | 7 => TrackSurface::Concrete,
            8 | 9 => TrackSurface::RacingDirt,
            10 | 11 => TrackSurface::Paint,
            12..=15 => TrackSurface::Rumble,
            16..=19 => TrackSurface::Grass,
            20..=23 => TrackSurface::Dirt,
            24 => TrackSurface::Sand,
            25..=28 => TrackSurface::Gravel,
            29 => TrackSurface::Grasscrete,
            30 => TrackSurface::Astroturf,
            _ => TrackSurface::Unknown,
        });

        let vehicle = Some(VehicleData {
            speed,
            rpm: get_f32("RPM").map(Rpm),
            max_rpm: None,
            idle_rpm: None,
            gear: get_i32("Gear").map(|g| g as i8),
            max_gears: None,
            throttle: get_f32("Throttle").map(Percentage::new),
            brake: get_f32("Brake").map(Percentage::new),
            clutch: get_f32("Clutch").map(Percentage::new),
            steering_angle: get_f32("SteeringWheelAngle").map(Radians),
            steering_torque: get_f32("SteeringWheelTorque").map(NewtonMeters),
            steering_torque_pct: get_f32("SteeringWheelPctTorque").map(Percentage::new),
            handbrake: None,
            on_track: get_bool("IsOnTrack"),
            in_garage: get_bool("IsInGarage"),
            track_surface,
        });

        // =================================================================
        // Engine
        // =================================================================
        let engine_warnings =
            get_u32("EngineWarnings").map(EngineWarnings::from_iracing_bits);

        let engine = Some(EngineData {
            water_temp: get_f32("WaterTemp").map(Celsius),
            oil_temp: get_f32("OilTemp").map(Celsius),
            oil_pressure: get_f32("OilPress").map(Kilopascals),
            oil_level: get_f32("OilLevel").map(Percentage::new),
            fuel_level: get_f32("FuelLevel").map(Liters),
            fuel_level_pct: get_f32("FuelLevelPct").map(Percentage::new),
            fuel_capacity: None,
            fuel_pressure: get_f32("FuelPress").map(Kilopascals),
            fuel_use_per_hour: get_f32("FuelUsePerHour").map(LitersPerHour),
            voltage: get_f32("Voltage").map(Volts),
            manifold_pressure: get_f32("ManifoldPress").map(Bar),
            warnings: engine_warnings,
        });

        // =================================================================
        // Wheels
        // =================================================================
        let wheels = Some(WheelData {
            front_left: self.extract_wheel(sample, "LF", true),
            front_right: self.extract_wheel(sample, "RF", false),
            rear_left: self.extract_wheel(sample, "LR", true),
            rear_right: self.extract_wheel(sample, "RR", false),
        });

        // =================================================================
        // Timing
        // =================================================================
        let timing = Some(TimingData {
            current_lap_time: get_f64("LapCurrentLapTime").map(|t| Seconds(t as f32)),
            last_lap_time: get_f64("LapLastLapTime").map(|t| Seconds(t as f32)),
            best_lap_time: get_f64("LapBestLapTime").map(|t| Seconds(t as f32)),
            best_n_lap_time: get_f64("LapBestNLapTime").map(|t| Seconds(t as f32)),
            best_n_lap_num: get_i32("LapBestNLapLap").map(|v| v as u32),
            sector_times: None,
            lap_number: get_i32("Lap").map(|l| l as u32),
            laps_completed: get_i32("LapCompleted").map(|l| l as u32),
            lap_distance: get_f32("LapDist").map(Meters),
            lap_distance_pct: get_f32("LapDistPct").map(Percentage::new),
            race_position: get_i32("PlayerCarPosition").map(|p| p as u32),
            class_position: get_i32("PlayerCarClassPosition").map(|p| p as u32),
            num_cars: None,
            delta_best: get_f32("LapDeltaToBestLap").map(Seconds),
            delta_best_ok: get_bool("LapDeltaToBestLap_OK"),
            delta_session_best: get_f32("LapDeltaToSessionBestLap").map(Seconds),
            delta_session_best_ok: get_bool("LapDeltaToSessionBestLap_OK"),
            delta_optimal: get_f32("LapDeltaToOptimalLap").map(Seconds),
            delta_optimal_ok: get_bool("LapDeltaToOptimalLap_OK"),
            estimated_lap_time: None,
            race_laps: None,
        });

        // =================================================================
        // Session
        // =================================================================
        let session_state = get_i32("SessionState").map(SessionState::from_iracing);
        let flags = get_u32("SessionFlags").map(FlagState::from_iracing_bits);
        let session_type = self.parse_session_type();

        let track_length = self
            .session_info
            .track_length
            .trim_end_matches(" km")
            .replace(',', ".")
            .parse::<f32>()
            .ok()
            .map(|km| Meters(km * 1000.0));

        let session = Some(SessionData {
            session_type,
            session_state,
            session_time: get_f64("SessionTime").map(|t| Seconds(t as f32)),
            session_time_remaining: get_f64("SessionTimeRemain").map(|t| Seconds(t as f32)),
            session_time_of_day: get_f32("SessionTimeOfDay").map(Seconds),
            session_laps: None,
            session_laps_remaining: get_i32("SessionLapsRemainEx").map(|l| l as u32),
            flags,
            track_name: Some(self.session_info.track_display_name.clone())
                .filter(|s| !s.is_empty()),
            track_config: Some(self.session_info.track_config_name.clone())
                .filter(|s| !s.is_empty()),
            track_length,
            track_type: None,
            car_name: Some(self.session_info.car_name.clone()).filter(|s| !s.is_empty()),
            car_class: None,
        });

        // =================================================================
        // Weather
        // =================================================================
        let weather = Some(WeatherData {
            air_temp: get_f32("AirTemp").map(Celsius),
            track_temp: get_f32("TrackTempCrew").map(Celsius),
            air_pressure: get_f32("AirPressure").map(Pascals),
            air_density: get_f32("AirDensity").map(KilogramsPerCubicMeter),
            humidity: get_f32("RelativeHumidity").map(|h| Percentage::new(h / 100.0)),
            wind_speed: get_f32("WindVel").map(MetersPerSecond),
            wind_direction: get_f32("WindDir").map(Radians),
            fog_level: get_f32("FogLevel").map(Percentage::new),
            precipitation: None,
            track_wetness: None,
            skies: get_i32("Skies").map(|s| match s {
                0 => "Clear".to_string(),
                1 => "Partly Cloudy".to_string(),
                2 => "Mostly Cloudy".to_string(),
                3 => "Overcast".to_string(),
                _ => format!("Unknown({})", s),
            }),
            declared_wet: None,
        });

        // =================================================================
        // Pit
        // =================================================================
        let requested_services = Some(PitServices {
            fuel_to_add: get_f32("dpFuelFill").map(Liters),
            change_tyre_fl: get_f32("dpLFTireChange").is_some_and(|v| v > 0.0),
            change_tyre_fr: get_f32("dpRFTireChange").is_some_and(|v| v > 0.0),
            change_tyre_rl: get_f32("dpLRTireChange").is_some_and(|v| v > 0.0),
            change_tyre_rr: get_f32("dpRRTireChange").is_some_and(|v| v > 0.0),
            windshield_tearoff: get_f32("dpWindshieldTearoff").is_some_and(|v| v > 0.0),
            fast_repair: get_f32("dpFastRepair").is_some_and(|v| v > 0.0),
            tyre_pressure_fl: get_f32("dpLFTireColdPress").map(Kilopascals),
            tyre_pressure_fr: get_f32("dpRFTireColdPress").map(Kilopascals),
            tyre_pressure_rl: get_f32("dpLRTireColdPress").map(Kilopascals),
            tyre_pressure_rr: get_f32("dpRRTireColdPress").map(Kilopascals),
        });

        let pit = Some(PitData {
            on_pit_road: get_bool("OnPitRoad"),
            pit_active: get_bool("PitstopActive"),
            pit_service_status: get_i32("PlayerCarPitSvStatus").map(|v| v as u32),
            repair_time_left: get_f32("PitRepairLeft").map(Seconds),
            optional_repair_time_left: get_f32("PitOptRepairLeft").map(Seconds),
            fast_repair_available: get_i32("FastRepairAvailable").map(|v| v as u32),
            fast_repair_used: get_i32("FastRepairUsed").map(|v| v as u32),
            pit_speed_limit: None,
            requested_services,
        });

        // =================================================================
        // Electronics
        // =================================================================
        let electronics = Some(ElectronicsData {
            abs: get_f32("dcABS"),
            traction_control: get_f32("dcTractionControl"),
            traction_control_2: None,
            brake_bias: get_f32("dcBrakeBias").map(Percentage::new),
            anti_roll_front: None,
            anti_roll_rear: None,
            drs_status: get_i32("DRS_Status").map(|v| v as u32),
            push_to_pass_status: None,
            push_to_pass_count: None,
            throttle_shape: None,
        });

        // =================================================================
        // Extras: every unmapped variable with iracing/ prefix
        // =================================================================
        let mut extras = HashMap::new();
        let mapped_set: std::collections::HashSet<&str> =
            Self::MAPPED_VARS.iter().copied().collect();

        for (name, value) in sample {
            if mapped_set.contains(name.as_str()) {
                continue;
            }
            // Skip CarIdx arrays (large per-car arrays, already in competitors)
            if name.starts_with("CarIdx") {
                continue;
            }
            let key = format!("iracing/{}", name);
            extras.insert(key, Self::var_value_to_json(value));
        }

        TelemetryFrame {
            timestamp: Utc::now(),
            game: "iRacing Replay".to_string(),
            tick,
            motion,
            vehicle,
            engine,
            wheels,
            timing,
            session,
            weather,
            pit,
            electronics,
            damage: None,
            competitors: None,
            driver: None,
            extras,
        }
    }

    /// Extract per-wheel data.
    /// `prefix` is "LF", "RF", "LR", or "RR".
    /// `is_left_side` determines inner/outer mapping for temperatures.
    fn extract_wheel(
        &self,
        sample: &HashMap<String, VarValue>,
        prefix: &str,
        is_left_side: bool,
    ) -> WheelInfo {
        let get_f32 = |suffix: &str| -> Option<f32> {
            let key = format!("{}{}", prefix, suffix);
            sample.get(&key).and_then(|v| v.as_f32())
        };

        // Inner/outer mapping: for left wheels, CL=outer edge, CR=inner edge.
        // For right wheels, CL=inner edge, CR=outer edge.
        let (surface_temp_inner, surface_temp_outer) = if is_left_side {
            (get_f32("tempCR").map(Celsius), get_f32("tempCL").map(Celsius))
        } else {
            (get_f32("tempCL").map(Celsius), get_f32("tempCR").map(Celsius))
        };

        let (carcass_temp_inner, carcass_temp_outer) = if is_left_side {
            (get_f32("tempR").map(Celsius), get_f32("tempL").map(Celsius))
        } else {
            (get_f32("tempL").map(Celsius), get_f32("tempR").map(Celsius))
        };

        WheelInfo {
            suspension_travel: get_f32("shockDefl").map(Meters),
            suspension_travel_avg: get_f32("shockDeflST").map(Meters),
            shock_velocity: get_f32("shockVel").map(MetersPerSecond),
            shock_velocity_avg: get_f32("shockVelST").map(MetersPerSecond),
            ride_height: get_f32("rideHeight").map(Meters),
            tyre_pressure: get_f32("pressure").map(Kilopascals),
            tyre_cold_pressure: get_f32("coldPressure").map(Kilopascals),
            surface_temp_inner,
            surface_temp_middle: get_f32("tempCM").map(Celsius),
            surface_temp_outer,
            carcass_temp_inner,
            carcass_temp_middle: get_f32("tempM").map(Celsius),
            carcass_temp_outer,
            tyre_wear: get_f32("wearL").map(Percentage::new),
            wheel_speed: get_f32("speed").map(RadiansPerSecond),
            slip_ratio: None,
            slip_angle: None,
            load: None,
            brake_line_pressure: get_f32("brakeLinePress").map(Kilopascals),
            brake_temp: None,
            tyre_compound: None,
        }
    }

    fn parse_session_type(&self) -> Option<SessionType> {
        let st = self.session_info.session_type.to_lowercase();
        if st.contains("race") {
            Some(SessionType::Race)
        } else if st.contains("qualify") || st.contains("qual") {
            Some(SessionType::Qualifying)
        } else if st.contains("practice") {
            Some(SessionType::Practice)
        } else if st.contains("time trial") || st.contains("timetrial") {
            Some(SessionType::TimeTrial)
        } else if st.contains("hotlap") {
            Some(SessionType::Hotlap)
        } else if st.contains("warmup") || st.contains("warm up") {
            Some(SessionType::Warmup)
        } else if !st.is_empty() {
            Some(SessionType::Other)
        } else {
            None
        }
    }
}

// ============================================================================
// Binary reading helpers
// ============================================================================

fn read_null_terminated_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}

fn read_scalar_value(buf: &[u8], offset: usize, var_type: VarType) -> Option<VarValue> {
    match var_type {
        VarType::Char => {
            if offset < buf.len() {
                Some(VarValue::Char(buf[offset]))
            } else {
                None
            }
        }
        VarType::Bool => {
            if offset < buf.len() {
                Some(VarValue::Bool(buf[offset] != 0))
            } else {
                None
            }
        }
        VarType::Int => {
            if offset + 4 <= buf.len() {
                Some(VarValue::Int(i32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::BitField => {
            if offset + 4 <= buf.len() {
                Some(VarValue::BitField(u32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::Float => {
            if offset + 4 <= buf.len() {
                Some(VarValue::Float(f32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::Double => {
            if offset + 8 <= buf.len() {
                Some(VarValue::Double(f64::from_le_bytes(
                    buf[offset..offset + 8].try_into().ok()?,
                )))
            } else {
                None
            }
        }
    }
}

fn read_array_value(
    buf: &[u8],
    offset: usize,
    var_type: VarType,
    count: usize,
) -> Option<VarValue> {
    match var_type {
        VarType::Char | VarType::Bool => {
            if offset + count <= buf.len() {
                Some(VarValue::CharArray(buf[offset..offset + count].to_vec()))
            } else {
                None
            }
        }
        VarType::Int | VarType::BitField => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 4;
                if off + 4 <= buf.len() {
                    vals.push(i32::from_le_bytes(
                        buf[off..off + 4].try_into().ok()?,
                    ));
                }
            }
            Some(VarValue::IntArray(vals))
        }
        VarType::Float => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 4;
                if off + 4 <= buf.len() {
                    vals.push(f32::from_le_bytes(
                        buf[off..off + 4].try_into().ok()?,
                    ));
                }
            }
            Some(VarValue::FloatArray(vals))
        }
        VarType::Double => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 8;
                if off + 8 <= buf.len() {
                    vals.push(f64::from_le_bytes(
                        buf[off..off + 8].try_into().ok()?,
                    ));
                }
            }
            Some(VarValue::DoubleArray(vals))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_var_type_from_i32() {
        assert_eq!(VarType::from_i32(0).unwrap(), VarType::Char);
        assert_eq!(VarType::from_i32(1).unwrap(), VarType::Bool);
        assert_eq!(VarType::from_i32(2).unwrap(), VarType::Int);
        assert_eq!(VarType::from_i32(3).unwrap(), VarType::BitField);
        assert_eq!(VarType::from_i32(4).unwrap(), VarType::Float);
        assert_eq!(VarType::from_i32(5).unwrap(), VarType::Double);
        assert!(VarType::from_i32(6).is_err());
    }

    #[test]
    fn test_var_value_conversions() {
        assert_eq!(VarValue::Float(1.5).as_f32(), Some(1.5));
        assert_eq!(VarValue::Double(2.5).as_f64(), Some(2.5));
        assert_eq!(VarValue::Int(42).as_i32(), Some(42));
        assert_eq!(VarValue::Bool(true).as_bool(), Some(true));
        assert_eq!(VarValue::BitField(0xFF).as_u32(), Some(0xFF));
    }

    #[test]
    fn test_read_null_terminated_string() {
        let buf = b"hello\0\0\0\0\0";
        assert_eq!(read_null_terminated_string(buf), "hello");

        let buf2 = b"no null here!!!!";
        assert_eq!(read_null_terminated_string(buf2), "no null here!!!!");
    }

    #[test]
    fn test_session_info_from_yaml() {
        let yaml = r#"---
WeekendInfo:
 TrackName: spielberg gp
 TrackDisplayName: Red Bull Ring
 TrackConfigName: Grand Prix
 TrackLength: 4.28 km
 DriverInfo:
 DriverCarIdx: 5
 Drivers:
 - CarIdx: 0
   UserName: Test Driver
   CarScreenName: Formula Test
SessionInfo:
 Sessions:
 - SessionNum: 0
   SessionType: Lone Qualify
"#;
        let info = IbtSessionInfo::from_yaml(yaml).unwrap();
        assert_eq!(info.track_name, "spielberg gp");
        assert_eq!(info.track_display_name, "Red Bull Ring");
        assert_eq!(info.driver_car_idx, 5);
        assert_eq!(info.driver_name, "Test Driver");
        assert_eq!(info.car_screen_name, "Formula Test");
        assert_eq!(info.session_type, "Lone Qualify");
    }

    #[test]
    fn test_read_scalar_values() {
        let mut buf = vec![0u8; 32];
        let val: f32 = 42.5;
        buf[0..4].copy_from_slice(&val.to_le_bytes());
        let ival: i32 = -123;
        buf[4..8].copy_from_slice(&ival.to_le_bytes());
        let dval: f64 = 99.99;
        buf[8..16].copy_from_slice(&dval.to_le_bytes());
        buf[16] = 1;

        match read_scalar_value(&buf, 0, VarType::Float) {
            Some(VarValue::Float(v)) => assert!((v - 42.5).abs() < 0.001),
            _ => panic!("Expected Float"),
        }
        match read_scalar_value(&buf, 4, VarType::Int) {
            Some(VarValue::Int(v)) => assert_eq!(v, -123),
            _ => panic!("Expected Int"),
        }
        match read_scalar_value(&buf, 8, VarType::Double) {
            Some(VarValue::Double(v)) => assert!((v - 99.99).abs() < 0.001),
            _ => panic!("Expected Double"),
        }
        match read_scalar_value(&buf, 16, VarType::Bool) {
            Some(VarValue::Bool(v)) => assert!(v),
            _ => panic!("Expected Bool"),
        }
    }

    // ========================================================================
    // Integration test: load the real fixtures/race.ibt file
    // ========================================================================

    fn fixture_path() -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../fixtures/race.ibt")
    }

    fn has_fixture() -> bool {
        fixture_path().exists()
    }

    #[test]
    fn test_ibt_open_and_header() {
        if !has_fixture() { return; }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        assert_eq!(ibt.header.ver, 2);
        assert_eq!(ibt.header.tick_rate, 60);
        assert_eq!(ibt.header.num_vars, 267);
        assert_eq!(ibt.disk_sub_header.session_record_count, 16263);
        assert_eq!(ibt.record_count(), 16263);
        let duration = ibt.duration_secs();
        assert!(duration > 270.0 && duration < 272.0,
            "Expected ~271s duration, got {duration}");
    }

    #[test]
    fn test_ibt_session_info_yaml() {
        if !has_fixture() { return; }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");
        let info = &ibt.session_info;
        assert_eq!(info.track_display_name, "Red Bull Ring");
        assert_eq!(info.session_type, "Lone Qualify");
    }

    #[test]
    fn test_ibt_read_and_convert_frame() {
        if !has_fixture() { return; }
        let mut ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        // Read a frame ~30s in where car is likely on track
        let idx = 1800.min(ibt.record_count() - 1);
        let sample = ibt.read_sample(idx).expect("Failed to read sample");
        let frame = ibt.sample_to_frame(&sample);

        assert_eq!(frame.game, "iRacing Replay");

        // Nested vehicle data
        let vehicle = frame.vehicle.as_ref().expect("vehicle should be populated");
        assert!(vehicle.speed.is_some());
        assert!(vehicle.rpm.is_some());
        assert!(vehicle.gear.is_some());
        assert!(vehicle.throttle.is_some());
        assert!(vehicle.brake.is_some());

        // Nested motion data
        let motion = frame.motion.as_ref().expect("motion should be populated");
        assert!(motion.velocity.is_some());
        assert!(motion.g_force.is_some());

        // Nested engine data
        let engine = frame.engine.as_ref().expect("engine should be populated");
        assert!(engine.water_temp.is_some());

        // Nested timing data
        let timing = frame.timing.as_ref().expect("timing should be populated");
        assert!(timing.lap_number.is_some());

        // Session data
        let session = frame.session.as_ref().expect("session should be populated");
        assert_eq!(session.track_name.as_deref(), Some("Red Bull Ring"));
        assert_eq!(session.session_type, Some(SessionType::Qualifying));

        // Wheels
        let wheels = frame.wheels.as_ref().expect("wheels should be populated");
        assert!(wheels.front_left.suspension_travel.is_some());
        assert!(wheels.front_right.tyre_pressure.is_some());
    }

    #[test]
    fn test_ibt_frame_values_are_sane() {
        if !has_fixture() { return; }
        let mut ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        for &idx in &[0, 1000, 5000, 10000, ibt.record_count() - 1] {
            if idx >= ibt.record_count() { continue; }
            let sample = ibt.read_sample(idx).unwrap();
            let frame = ibt.sample_to_frame(&sample);

            if let Some(ref v) = frame.vehicle {
                if let Some(speed) = v.speed {
                    assert!(speed.0 >= 0.0 && speed.0 < 120.0,
                        "Frame {idx}: Speed {:.1} m/s out of range", speed.0);
                }
                if let Some(rpm) = v.rpm {
                    assert!(rpm.0 >= 0.0 && rpm.0 < 20000.0);
                }
            }
            if let Some(ref m) = frame.motion {
                if let Some(ref g) = m.g_force {
                    assert!(g.x.0.abs() < 10.0);
                    assert!(g.z.0.abs() < 10.0);
                }
            }
        }
    }

    #[test]
    fn test_ibt_sequential_read_consistency() {
        if !has_fixture() { return; }
        let mut ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        let first = ibt.read_sample(0).unwrap().get("SessionTime").unwrap().as_f64().unwrap();
        let sixtieth = ibt.read_sample(59).unwrap().get("SessionTime").unwrap().as_f64().unwrap();
        let elapsed = sixtieth - first;
        assert!((elapsed - 1.0).abs() < 0.1,
            "60 frames at 60Hz should span ~1 second, got {elapsed:.3}s");
    }
}
