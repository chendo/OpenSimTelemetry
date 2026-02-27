/* ==================== Constants ==================== */
const LAYOUT_KEY = 'ost-dashboard-layout';
const LAYOUT_VERSION_KEY = 'ost-dashboard-version';
const GRAPHS_KEY = 'ost-dashboard-graphs';
const LAYOUT_VERSION = '4'; // Bumped for GridStack migration (0-based coords)
const RAD2DEG = 180 / Math.PI;
const BUFFER_MAX = 3600;

// Global crosshair state: when hovering any graph, all graphs show a vertical cursor at the same timestamp
const crosshair = { t: null };
let _uiDirty = false;
function requestRedraw() { _uiDirty = true; }
let streamPaused = false;

/* ==================== Graph Metrics Registry ==================== */
const GRAPH_METRICS = {
    speed:       { label: 'Speed',      color: '#00d68f', unit: 'km/h',  norm: 'autoscale', extract: f => (f.vehicle?.speed ?? 0) * 3.6 },
    rpm:         { label: 'RPM',        color: '#ff6b6b', unit: 'rpm',   norm: 'autoscale', extract: f => f.vehicle?.rpm ?? 0 },
    throttle:    { label: 'Throttle',   color: '#4ecdc4', unit: '%',     norm: 'pct',       extract: f => f.vehicle?.throttle ?? 0 },
    brake:       { label: 'Brake',      color: '#ff4757', unit: '%',     norm: 'pct',       extract: f => f.vehicle?.brake ?? 0 },
    steering:    { label: 'Steering',   color: '#ffa502', unit: '%',     norm: 'centered',  extract: f => f.vehicle?.steering_angle ?? 0 },
    lat_g:       { label: 'Lateral G',  color: '#a855f6', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.x ?? 0 },
    long_g:      { label: 'Long G',     color: '#ec4899', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.z ?? 0 },
    vert_g:      { label: 'Vert G',     color: '#6366f1', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.y ?? 0 },
    pitch:       { label: 'Pitch',      color: '#f97316', unit: 'deg',   norm: 'centered',  extract: f => (f.motion?.rotation?.x ?? 0) * RAD2DEG },
    yaw_rate:    { label: 'Yaw Rate',   color: '#eab308', unit: 'deg/s', norm: 'centered',  extract: f => (f.motion?.angular_velocity?.y ?? 0) * RAD2DEG },
    roll:        { label: 'Roll',       color: '#14b8a6', unit: 'deg',   norm: 'centered',  extract: f => (f.motion?.rotation?.z ?? 0) * RAD2DEG },
};
const GRAPH_METRIC_KEYS = Object.keys(GRAPH_METRICS);

/* ==================== Field Unit Metadata ==================== */
// Maps field paths to { unit, norm } for arbitrary field plotting.
// Lookup: exact match first, then suffix match (*.field_name).
const FIELD_UNIT_MAP = {
    // Exact paths with special handling
    'vehicle.speed':          { unit: 'm/s',  norm: 'autoscale' },
    'vehicle.gear':           { unit: '',     norm: 'autoscale' },
    'vehicle.max_gears':      { unit: '',     norm: 'autoscale' },
    // Suffix patterns (matched when exact fails)
    '*.rpm':                  { unit: 'rpm',  norm: 'autoscale' },
    '*.max_rpm':              { unit: 'rpm',  norm: 'autoscale' },
    '*.idle_rpm':             { unit: 'rpm',  norm: 'autoscale' },
    '*.throttle':             { unit: '%',    norm: 'pct' },
    '*.brake':                { unit: '%',    norm: 'pct' },
    '*.clutch':               { unit: '%',    norm: 'pct' },
    '*.handbrake':            { unit: '%',    norm: 'pct' },
    '*.steering_angle':       { unit: 'rad',  norm: 'centered' },
    '*.steering_torque':      { unit: 'Nm',   norm: 'centered' },
    '*.steering_torque_pct':  { unit: '%',    norm: 'pct' },
    // Temperatures
    '*.water_temp':           { unit: '\u00B0C', norm: 'autoscale' },
    '*.oil_temp':             { unit: '\u00B0C', norm: 'autoscale' },
    '*.air_temp':             { unit: '\u00B0C', norm: 'autoscale' },
    '*.track_temp':           { unit: '\u00B0C', norm: 'autoscale' },
    '*.surface_temp_inner':   { unit: '\u00B0C', norm: 'autoscale' },
    '*.surface_temp_middle':  { unit: '\u00B0C', norm: 'autoscale' },
    '*.surface_temp_outer':   { unit: '\u00B0C', norm: 'autoscale' },
    '*.carcass_temp_inner':   { unit: '\u00B0C', norm: 'autoscale' },
    '*.carcass_temp_middle':  { unit: '\u00B0C', norm: 'autoscale' },
    '*.carcass_temp_outer':   { unit: '\u00B0C', norm: 'autoscale' },
    '*.brake_temp':           { unit: '\u00B0C', norm: 'autoscale' },
    // Pressure
    '*.tyre_pressure':        { unit: 'kPa', norm: 'autoscale' },
    '*.tyre_cold_pressure':   { unit: 'kPa', norm: 'autoscale' },
    '*.oil_pressure':         { unit: 'kPa', norm: 'autoscale' },
    '*.fuel_pressure':        { unit: 'kPa', norm: 'autoscale' },
    '*.brake_line_pressure':  { unit: 'kPa', norm: 'autoscale' },
    '*.air_pressure':         { unit: 'Pa',  norm: 'autoscale' },
    '*.manifold_pressure':    { unit: 'bar', norm: 'autoscale' },
    // Suspension / distance
    '*.suspension_travel':     { unit: 'm',   norm: 'autoscale' },
    '*.suspension_travel_avg': { unit: 'm',   norm: 'autoscale' },
    '*.ride_height':           { unit: 'm',   norm: 'autoscale' },
    '*.lap_distance':          { unit: 'm',   norm: 'autoscale' },
    // Velocity
    '*.shock_velocity':        { unit: 'm/s', norm: 'centered' },
    '*.shock_velocity_avg':    { unit: 'm/s', norm: 'centered' },
    '*.wind_speed':            { unit: 'm/s', norm: 'autoscale' },
    '*.wheel_speed':           { unit: 'rad/s', norm: 'autoscale' },
    // Forces
    '*.load':                  { unit: 'N',   norm: 'autoscale' },
    '*.g_force.x':             { unit: 'G',   norm: 'centered' },
    '*.g_force.y':             { unit: 'G',   norm: 'centered' },
    '*.g_force.z':             { unit: 'G',   norm: 'centered' },
    // Rotation
    '*.rotation.x':            { unit: 'rad', norm: 'centered' },
    '*.rotation.y':            { unit: 'rad', norm: 'centered' },
    '*.rotation.z':            { unit: 'rad', norm: 'centered' },
    '*.angular_velocity.x':    { unit: 'rad/s', norm: 'centered' },
    '*.angular_velocity.y':    { unit: 'rad/s', norm: 'centered' },
    '*.angular_velocity.z':    { unit: 'rad/s', norm: 'centered' },
    '*.angular_acceleration.x':{ unit: 'rad/s\u00B2', norm: 'centered' },
    '*.angular_acceleration.y':{ unit: 'rad/s\u00B2', norm: 'centered' },
    '*.angular_acceleration.z':{ unit: 'rad/s\u00B2', norm: 'centered' },
    '*.acceleration.x':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.acceleration.y':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.acceleration.z':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.position.x':            { unit: 'm', norm: 'autoscale' },
    '*.position.y':            { unit: 'm', norm: 'autoscale' },
    '*.position.z':            { unit: 'm', norm: 'autoscale' },
    '*.velocity.x':            { unit: 'm/s', norm: 'centered' },
    '*.velocity.y':            { unit: 'm/s', norm: 'centered' },
    '*.velocity.z':            { unit: 'm/s', norm: 'centered' },
    '*.wind_direction':        { unit: 'rad', norm: 'autoscale' },
    // Percentages
    '*.tyre_wear':             { unit: '%', norm: 'pct' },
    '*.oil_level':             { unit: '%', norm: 'pct' },
    '*.fuel_level_pct':        { unit: '%', norm: 'pct' },
    '*.lap_distance_pct':      { unit: '%', norm: 'pct' },
    '*.humidity':              { unit: '%', norm: 'pct' },
    '*.fog_level':             { unit: '%', norm: 'pct' },
    '*.precipitation':         { unit: '%', norm: 'pct' },
    '*.brake_bias':            { unit: '%', norm: 'pct' },
    '*.front':                 { unit: '%', norm: 'pct' },  // damage
    '*.rear':                  { unit: '%', norm: 'pct' },
    '*.left':                  { unit: '%', norm: 'pct' },
    '*.right':                 { unit: '%', norm: 'pct' },
    // Fuel / volume
    '*.fuel_level':            { unit: 'L',   norm: 'autoscale' },
    '*.fuel_capacity':         { unit: 'L',   norm: 'autoscale' },
    '*.fuel_use_per_hour':     { unit: 'L/h', norm: 'autoscale' },
    '*.fuel_to_add':           { unit: 'L',   norm: 'autoscale' },
    // Electrical
    '*.voltage':               { unit: 'V',   norm: 'autoscale' },
    // Time
    '*.current_lap_time':      { unit: 's',   norm: 'autoscale' },
    '*.last_lap_time':         { unit: 's',   norm: 'autoscale' },
    '*.best_lap_time':         { unit: 's',   norm: 'autoscale' },
    '*.best_n_lap_time':       { unit: 's',   norm: 'autoscale' },
    '*.session_time':          { unit: 's',   norm: 'autoscale' },
    '*.session_time_remaining':{ unit: 's',   norm: 'autoscale' },
    '*.delta_best':            { unit: 's',   norm: 'centered' },
    '*.delta_session_best':    { unit: 's',   norm: 'centered' },
    '*.delta_optimal':         { unit: 's',   norm: 'centered' },
    '*.repair_time_left':      { unit: 's',   norm: 'autoscale' },
    '*.optional_repair_time_left': { unit: 's', norm: 'autoscale' },
    // Slip
    '*.slip_ratio':            { unit: '', norm: 'centered' },
    '*.slip_angle':            { unit: 'rad', norm: 'centered' },
    // Density
    '*.air_density':           { unit: 'kg/m\u00B3', norm: 'autoscale' },
};

function getFieldUnitInfo(path) {
    if (FIELD_UNIT_MAP[path]) return FIELD_UNIT_MAP[path];
    const parts = path.split('.');
    for (let i = 1; i < parts.length; i++) {
        const suffix = '*.' + parts.slice(i).join('.');
        if (FIELD_UNIT_MAP[suffix]) return FIELD_UNIT_MAP[suffix];
    }
    return { unit: '', norm: 'autoscale' };
}

// Format a numeric field value for display with unit conversion
// Some fields are stored in SI units but displayed in friendlier units
const METERS_TO_MM_FIELDS = /suspension_travel|ride_height/;
function formatFieldValue(path, value) {
    if (typeof value !== 'number') return { text: JSON.stringify(value), unit: '' };
    const info = getFieldUnitInfo(path);
    const leaf = path.split('.').pop();
    // Meters → mm for suspension/ride height
    if (info.unit === 'm' && METERS_TO_MM_FIELDS.test(leaf)) return { text: (value * 1000).toFixed(2), unit: 'mm' };
    // 0-1 → % for percentage fields
    if (info.unit === '%' && info.norm === 'pct') return { text: (value * 100).toFixed(1), unit: '%' };
    // Smart precision: more decimals for small numbers
    const av = Math.abs(value);
    const text = av >= 1000 ? value.toFixed(0) : av >= 10 ? value.toFixed(1) : av >= 0.01 ? value.toFixed(3) : value.toFixed(4);
    return { text, unit: info.unit };
}

function resolveFieldPathParts(obj, parts) {
    let cur = obj;
    for (let i = 0; i < parts.length; i++) {
        if (cur == null || typeof cur !== 'object') return null;
        cur = cur[parts[i]];
    }
    return typeof cur === 'number' ? cur : null;
}

const CUSTOM_COLORS = [
    '#22d3ee', '#f472b6', '#a3e635', '#fb923c', '#818cf8',
    '#e879f9', '#34d399', '#fbbf24', '#f87171', '#38bdf8',
    '#c084fc', '#2dd4bf', '#facc15', '#fb7185', '#60a5fa',
];
let _nextCustomColorIdx = 0;
function nextCustomColor() {
    return CUSTOM_COLORS[(_nextCustomColorIdx++) % CUSTOM_COLORS.length];
}

const LABEL_ABBREVS = {
    front_left: 'FL', front_right: 'FR', rear_left: 'RL', rear_right: 'RR',
    surface_temp: 'Surf Temp', carcass_temp: 'Carc Temp',
    angular_velocity: 'Ang Vel', angular_acceleration: 'Ang Accel',
    g_force: 'G-Force', brake_line_pressure: 'Brake Press',
    suspension_travel: 'Susp Travel', suspension_travel_avg: 'Susp Avg',
    shock_velocity: 'Shock Vel', shock_velocity_avg: 'Shock Vel Avg',
    tyre_pressure: 'Tyre Press', tyre_cold_pressure: 'Cold Press',
    fuel_use_per_hour: 'Fuel Rate', fuel_level_pct: 'Fuel %',
    lap_distance_pct: 'Lap %', steering_angle: 'Steer',
    steering_torque: 'Steer Torque', steering_torque_pct: 'Steer %',
    manifold_pressure: 'Manifold', oil_pressure: 'Oil Press',
    fuel_pressure: 'Fuel Press', water_temp: 'Water Temp',
    oil_temp: 'Oil Temp', brake_temp: 'Brake Temp',
    current_lap_time: 'Cur Lap', last_lap_time: 'Last Lap',
    best_lap_time: 'Best Lap', delta_best: '\u0394 Best',
    delta_session_best: '\u0394 Sess Best', delta_optimal: '\u0394 Optimal',
};

function deriveLabel(path) {
    const parts = path.split('.');
    return parts.map(p =>
        LABEL_ABBREVS[p] || p.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')
    ).join(' ');
}

