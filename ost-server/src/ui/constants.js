/* ==================== Constants ==================== */
const LAYOUT_KEY = 'ost-dashboard-layout';
const LAYOUT_VERSION_KEY = 'ost-dashboard-version';
const GRAPHS_KEY = 'ost-dashboard-graphs';
const LAYOUT_VERSION = '11'; // Remove batchUpdate to fix width=3 quarters
const BUFFER_MAX = 3600;

// History buffer settings
const HISTORY_DURATION_KEY = 'ost-history-duration-secs';
const DEFAULT_HISTORY_SECS = 600;
const HISTORY_DURATION_OPTIONS = [
    { secs: 300,  label: '5 min' },
    { secs: 600,  label: '10 min' },
    { secs: 900,  label: '15 min' },
    { secs: 1800, label: '30 min' },
    { secs: 3600, label: '60 min' },
];

// Global crosshair state: when hovering any graph, all graphs show a vertical cursor at the same timestamp
const crosshair = { t: null };
let _uiDirty = false;
let _wakeupRender = null; // assigned by renderLoop setup in app.js
function requestRedraw() { _uiDirty = true; _wakeupRender?.(); }
let streamPaused = false;

/* ==================== Semantic Metric Colours ==================== */
// Colour scheme conventions:
//   Driver inputs: Throttle=green, Brake=red, Clutch=blue, ABS=reddish
//   Axes: X=red, Y=green, Z=blue
//   Motion categories use the same axis hue but shifted to differentiate:
//     G-Force:      bright primary (red, green, blue)
//     Rotation:     warm-shifted  (orange, lime, cyan)
//     Rates:        lighter warm  (lt orange, lt lime, lt cyan)
//     Velocity:     pastel        (pastel red, pastel green, pastel blue)
//     Acceleration: dark primary  (dk red, dk green, dk blue)
//     Ang. Accel:   dark warm     (dk orange, dk lime, dk cyan)
//     Position:     light primary (lt red, lt green, lt blue)
const METRIC_COLORS = {
    // Driver inputs
    'vehicle.throttle':           '#22c55e',  // Green
    'vehicle.brake':              '#ef4444',  // Red
    'vehicle.clutch':             '#3b82f6',  // Blue
    'vehicle.steering_angle':     '#f59e0b',  // Amber
    'vehicle.handbrake':          '#fbbf24',  // Yellow (brake-adjacent)
    'electronics.abs_active':     '#f87171',  // Light red (brake-related)
    'electronics.tc_active':      '#fb923c',  // Orange (traction)
    // Vehicle
    'vehicle.speed':              '#38bdf8',  // Sky blue
    'vehicle.rpm':                '#a855f6',  // Purple
    // G-Force — bright primary RGB
    'motion.g_force.x':           '#ef4444',  // Red (X — lateral)
    'motion.g_force.y':           '#22c55e',  // Green (Y — vertical)
    'motion.g_force.z':           '#3b82f6',  // Blue (Z — longitudinal)
    // Rotation — warm-shifted
    'motion.rotation.x':          '#f97316',  // Orange (X — pitch)
    'motion.rotation.y':          '#84cc16',  // Lime (Y — yaw)
    'motion.rotation.z':          '#06b6d4',  // Cyan (Z — roll)
    // Angular rates — lighter warm-shifted
    'motion.pitch_rate':          '#fb923c',  // Lt orange (X)
    'motion.yaw_rate':            '#a3e635',  // Lt lime (Y)
    'motion.roll_rate':           '#22d3ee',  // Lt cyan (Z)
    // Velocity — pastel RGB
    'motion.velocity.x':          '#fca5a5',  // Pastel red (X)
    'motion.velocity.y':          '#86efac',  // Pastel green (Y)
    'motion.velocity.z':          '#93c5fd',  // Pastel blue (Z)
    // Acceleration — dark RGB
    'motion.acceleration.x':      '#dc2626',  // Dk red (X)
    'motion.acceleration.y':      '#16a34a',  // Dk green (Y)
    'motion.acceleration.z':      '#2563eb',  // Dk blue (Z)
    // Angular acceleration — dark warm-shifted
    'motion.angular_acceleration.x': '#ea580c',  // Dk orange (X)
    'motion.angular_acceleration.y': '#65a30d',  // Dk lime (Y)
    'motion.angular_acceleration.z': '#0891b2',  // Dk cyan (Z)
    // Position — light RGB
    'motion.position.x':          '#f87171',  // Lt red (X)
    'motion.position.y':          '#4ade80',  // Lt green (Y)
    'motion.position.z':          '#60a5fa',  // Lt blue (Z)
};

function getMetricColor(path) {
    return METRIC_COLORS[path] || nextCustomColor();
}

/* ==================== Graph Metrics Registry ==================== */
const GRAPH_METRICS = {
    speed:       { label: 'Speed',      color: '#38bdf8', unit: 'm/s',   norm: 'autoscale', extract: f => f.vehicle?.speed ?? 0 },
    rpm:         { label: 'RPM',        color: '#a855f6', unit: 'rpm',   norm: 'autoscale', extract: f => f.vehicle?.rpm ?? 0 },
    throttle:    { label: 'Throttle',   color: '#22c55e', unit: '%',     norm: 'pct',       extract: f => f.vehicle?.throttle ?? 0 },
    brake:       { label: 'Brake',      color: '#ef4444', unit: '%',     norm: 'pct',       extract: f => f.vehicle?.brake ?? 0 },
    clutch:      { label: 'Clutch',     color: '#3b82f6', unit: '%',     norm: 'pct',       extract: f => f.vehicle?.clutch ?? 0 },
    steering:    { label: 'Steering',   color: '#f59e0b', unit: '\u00B0',  norm: 'centered',  extract: f => f.vehicle?.steering_angle ?? 0 },
    abs_active:  { label: 'ABS',        color: '#f87171', unit: '',      norm: 'boolean',   extract: f => f.electronics?.abs_active ?? false },
    lat_g:       { label: 'Lateral G',  color: '#ef4444', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.x ?? 0 },
    long_g:      { label: 'Long G',     color: '#3b82f6', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.z ?? 0 },
    vert_g:      { label: 'Vert G',     color: '#22c55e', unit: 'G',     norm: 'centered',  extract: f => f.motion?.g_force?.y ?? 0 },
    pitch:       { label: 'Pitch',      color: '#f97316', unit: '\u00B0',    norm: 'centered',  extract: f => f.motion?.rotation?.x ?? 0 },
    yaw_rate:    { label: 'Yaw Rate',   color: '#a3e635', unit: '\u00B0/s',  norm: 'centered',  extract: f => f.motion?.yaw_rate ?? 0 },
    roll:        { label: 'Roll',       color: '#06b6d4', unit: '\u00B0',    norm: 'centered',  extract: f => f.motion?.rotation?.z ?? 0 },
};
const GRAPH_METRIC_KEYS = Object.keys(GRAPH_METRICS);

/* ==================== Graph Presets ==================== */
const GRAPH_PRESETS = [
    { name: 'Pedals & Speed', metrics: ['vehicle.speed', 'vehicle.throttle', 'vehicle.brake', 'vehicle.clutch', 'vehicle.steering_angle'] },
    { name: 'G-Forces', metrics: ['motion.g_force.x', 'motion.g_force.z', 'motion.g_force.y'] },
    { name: 'Wheel Suspension', metrics: ['wheels.*.suspension_travel', 'wheels.*.shock_velocity', 'wheels.*.ride_height'] },
    { name: 'Tire Temps', metrics: ['wheels.*.surface_temp_*'] },
    { name: 'Tire Wear', metrics: ['wheels.*.tyre_wear'] },
    { name: 'Tire Pressure', metrics: ['wheels.*.tyre_pressure', 'wheels.*.tyre_cold_pressure'] },
    { name: 'Brake Temps', metrics: ['wheels.*.brake_temp'] },
    { name: 'Engine', metrics: ['vehicle.rpm', 'wheels.*.wheel_speed'] },
];

/* ==================== Metric Unit Metadata ==================== */
// Maps metric paths to { unit, norm } for arbitrary metric plotting.
// Lookup: exact match first, then suffix match (*.metric_name).
const METRIC_UNIT_MAP = {
    // Exact paths with special handling
    'vehicle.speed':          { unit: 'm/s', norm: 'autoscale' },
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
    '*.steering_angle':       { unit: '\u00B0',  norm: 'centered' },
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
    '*.air_pressure':         { unit: 'kPa', norm: 'autoscale' },
    '*.manifold_pressure':    { unit: 'bar', norm: 'autoscale' },
    // Suspension / distance
    '*.suspension_travel':     { unit: 'mm',  norm: 'autoscale' },
    '*.suspension_travel_avg': { unit: 'mm',  norm: 'autoscale' },
    '*.ride_height':           { unit: 'mm',  norm: 'autoscale' },
    '*.lap_distance':          { unit: 'm',   norm: 'autoscale' },
    // Velocity
    '*.shock_velocity':        { unit: 'mm/s', norm: 'centered' },
    '*.shock_velocity_avg':    { unit: 'mm/s', norm: 'centered' },
    '*.wind_speed':            { unit: 'm/s', norm: 'autoscale' },
    '*.wheel_speed':           { unit: 'rpm', norm: 'autoscale' },
    // Forces
    '*.load':                  { unit: 'N',   norm: 'autoscale' },
    '*.g_force.x':             { unit: 'G',   norm: 'centered' },
    '*.g_force.y':             { unit: 'G',   norm: 'centered' },
    '*.g_force.z':             { unit: 'G',   norm: 'centered' },
    // Rotation
    '*.rotation.x':            { unit: '\u00B0', norm: 'centered' },
    '*.rotation.y':            { unit: '\u00B0', norm: 'centered' },
    '*.rotation.z':            { unit: '\u00B0', norm: 'centered' },
    '*.pitch_rate':             { unit: '\u00B0/s', norm: 'centered' },
    '*.yaw_rate':               { unit: '\u00B0/s', norm: 'centered' },
    '*.roll_rate':              { unit: '\u00B0/s', norm: 'centered' },
    '*.angular_acceleration.x':{ unit: '\u00B0/s\u00B2', norm: 'centered' },
    '*.angular_acceleration.y':{ unit: '\u00B0/s\u00B2', norm: 'centered' },
    '*.angular_acceleration.z':{ unit: '\u00B0/s\u00B2', norm: 'centered' },
    '*.acceleration.x':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.acceleration.y':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.acceleration.z':        { unit: 'm/s\u00B2', norm: 'centered' },
    '*.position.x':            { unit: 'm', norm: 'autoscale' },
    '*.position.y':            { unit: 'm', norm: 'autoscale' },
    '*.position.z':            { unit: 'm', norm: 'autoscale' },
    '*.velocity.x':            { unit: 'm/s', norm: 'centered' },
    '*.velocity.y':            { unit: 'm/s', norm: 'centered' },
    '*.velocity.z':            { unit: 'm/s', norm: 'centered' },
    '*.wind_direction':        { unit: '\u00B0', norm: 'autoscale' },
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
    '*.slip_angle':            { unit: '\u00B0', norm: 'centered' },
    // Density
    '*.air_density':           { unit: 'kg/m\u00B3', norm: 'autoscale' },
};

const _metricUnitCache = new Map();
function getMetricUnitInfo(path) {
    let result = _metricUnitCache.get(path);
    if (result !== undefined) return result;
    if (METRIC_UNIT_MAP[path]) { result = METRIC_UNIT_MAP[path]; }
    else {
        result = { unit: '', norm: 'autoscale' };
        const parts = path.split('.');
        for (let i = 1; i < parts.length; i++) {
            const suffix = '*.' + parts.slice(i).join('.');
            if (METRIC_UNIT_MAP[suffix]) { result = METRIC_UNIT_MAP[suffix]; break; }
        }
    }
    _metricUnitCache.set(path, result);
    return result;
}

// Apply user unit preferences to a base unit and value.
// Returns { value, unit } with the converted value and display unit string.
// Handles unit families via prefix matching (e.g. '°' matches '°/s' and '°/s²').
function applyUnitPref(baseUnit, value) {
    if (!baseUnit) return { value, unit: baseUnit };
    const prefs = getUnitPrefs();
    // Two passes: exact match first, then prefix match for unit families
    for (let pass = 0; pass < 2; pass++) {
        for (const [sysKey] of Object.entries(UNIT_SYSTEMS)) {
            const selected = prefs[sysKey];
            const conv = UNIT_CONVERSIONS[selected];
            if (!conv) continue;
            if (pass === 0 && baseUnit === conv.from) {
                return {
                    value: value * conv.factor + (conv.offset || 0),
                    unit: selected,
                };
            }
            if (pass === 1 && baseUnit !== conv.from && baseUnit.startsWith(conv.from)) {
                const suffix = baseUnit.slice(conv.from.length);
                // Only match if suffix starts with a separator (/, ², etc.), not a letter
                if (suffix && /^[a-zA-Z]/.test(suffix)) continue;
                return {
                    value: value * conv.factor + (conv.offset || 0),
                    unit: selected + suffix,
                };
            }
        }
    }
    return { value, unit: baseUnit };
}

// Format a numeric metric value for display with unit conversion
function formatMetricValue(path, value) {
    if (typeof value !== 'number') return { text: JSON.stringify(value), unit: '' };
    const info = getMetricUnitInfo(path);
    // Apply multiplier from METRIC_UNIT_MAP (legacy, if any remain)
    if (info.multiplier) value = value * info.multiplier;
    // 0-1 → % for percentage metrics
    if (info.unit === '%' && info.norm === 'pct') return { text: (value * 100).toFixed(1), unit: '%' };
    // Apply user unit preferences
    const converted = applyUnitPref(info.unit, value);
    value = converted.value;
    const displayUnit = converted.unit;
    // Smart precision: more decimals for small numbers
    const av = Math.abs(value);
    const text = av >= 1000 ? value.toFixed(0) : av >= 10 ? value.toFixed(1) : av >= 0.01 ? value.toFixed(3) : value.toFixed(4);
    return { text, unit: displayUnit };
}

function resolveMetricPathParts(obj, parts) {
    let cur = obj;
    for (let i = 0; i < parts.length; i++) {
        if (cur == null || typeof cur !== 'object') return null;
        cur = cur[parts[i]];
    }
    return typeof cur === 'number' ? cur : null;
}

function resolveMetricPathRaw(obj, parts) {
    let cur = obj;
    for (let i = 0; i < parts.length; i++) {
        if (cur == null || typeof cur !== 'object') return undefined;
        cur = cur[parts[i]];
    }
    return cur;
}

// Hash a string to a deterministic HSL color for text/enum metric bars
function hashStringColor(str) {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
        hash = str.charCodeAt(i) + ((hash << 5) - hash);
    }
    const hue = ((hash % 360) + 360) % 360;
    return `hsl(${hue}, 70%, 55%)`;
}

/* ==================== Unit Preferences ==================== */
const UNIT_PREFS_KEY = 'ost-unit-preferences';
const UNIT_SYSTEMS = {
    speed:       { options: ['km/h', 'mph', 'm/s'],  default: 'km/h' },
    temperature: { options: ['\u00b0C', '\u00b0F'],   default: '\u00b0C' },
    pressure:    { options: ['kPa', 'psi', 'bar'],    default: 'kPa' },
    distance:    { options: ['mm', 'in'],             default: 'mm' },
    rotation:    { options: ['deg', 'rad'],            default: 'deg' },
};
const UNIT_CONVERSIONS = {
    'km/h':  { from: 'm/s', factor: 3.6 },
    'mph':   { from: 'm/s', factor: 2.23694 },
    'm/s':   { from: 'm/s', factor: 1 },
    '\u00b0C':  { from: '\u00b0C', factor: 1, offset: 0 },
    '\u00b0F':  { from: '\u00b0C', factor: 1.8, offset: 32 },
    'kPa':   { from: 'kPa', factor: 1 },
    'psi':   { from: 'kPa', factor: 0.145038 },
    'bar':   { from: 'kPa', factor: 0.01 },
    'mm':    { from: 'mm', factor: 1 },
    'in':    { from: 'mm', factor: 0.0393701 },
    'deg':   { from: '\u00b0', factor: 1 },
    'rad':   { from: '\u00b0', factor: Math.PI / 180 },
};

function getUnitPrefs() {
    try {
        const saved = JSON.parse(localStorage.getItem(UNIT_PREFS_KEY));
        return { ...Object.fromEntries(Object.entries(UNIT_SYSTEMS).map(([k, v]) => [k, v.default])), ...saved };
    } catch { return Object.fromEntries(Object.entries(UNIT_SYSTEMS).map(([k, v]) => [k, v.default])); }
}

function saveUnitPrefs(prefs) {
    localStorage.setItem(UNIT_PREFS_KEY, JSON.stringify(prefs));
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
    pitch_rate: 'Pitch Rate', yaw_rate: 'Yaw Rate', roll_rate: 'Roll Rate',
    angular_acceleration: 'Ang Accel',
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

/* ==================== Metric Aliases ==================== */
// Alternative search terms for renamed/technical metric paths
const METRIC_ALIASES = {
    'motion.pitch_rate': ['angular_velocity.x', 'pitch_velocity'],
    'motion.yaw_rate':   ['angular_velocity.y', 'yaw_velocity'],
    'motion.roll_rate':  ['angular_velocity.z', 'roll_velocity'],
};

/* ==================== Metric Filter Matching ==================== */
// Supports three modes:
//   - /pattern/flags  → regex (case-insensitive by default)
//   - contains *      → wildcard (* matches any non-dot chars within a path segment)
//   - otherwise       → case-insensitive substring match
// Also checks METRIC_ALIASES so renamed metrics are still discoverable.
function matchMetricFilter(path, filter) {
    if (!filter) return true;
    if (_matchFilter(path, filter)) return true;
    // Check aliases for this path
    const aliases = METRIC_ALIASES[path];
    if (aliases) {
        for (const alias of aliases) {
            if (_matchFilter(alias, filter)) return true;
        }
    }
    return false;
}

function _matchFilter(text, filter) {
    if (filter.startsWith('/')) {
        try {
            const end = filter.lastIndexOf('/');
            const pattern = end > 0 ? filter.slice(1, end) : filter.slice(1);
            if (!pattern) return true;
            const flags = end > 0 && end !== 0 ? filter.slice(end + 1) : 'i';
            return new RegExp(pattern, flags || 'i').test(text);
        } catch { return false; }
    }
    if (filter.includes('*')) {
        try {
            const parts = filter.split('*');
            const escaped = parts.map(p => p.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
            return new RegExp(escaped.join('[^.]*'), 'i').test(text);
        } catch { return false; }
    }
    return text.toLowerCase().includes(filter.toLowerCase());
}

const TOP_LEVEL_SECTIONS = new Set([
    'vehicle', 'motion', 'engine', 'wheels', 'timing', 'session',
    'weather', 'pit', 'electronics', 'damage',
    // Game-specific namespaces are also top-level (e.g. "iracing", "demo")
    'iracing', 'demo',
]);

function deriveLabel(path) {
    let parts = path.split('.');
    // Strip top-level section prefix and adapter namespace (e.g. "iRacing/")
    if (TOP_LEVEL_SECTIONS.has(parts[0])) parts = parts.slice(1);
    parts = parts.map(p => p.includes('/') ? p.split('/').pop() : p);

    const splitWord = (w) =>
        w.replace(/([a-z])([A-Z])/g, '$1 $2')
         .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
         .split(' ')
         .map(s => s.charAt(0).toUpperCase() + s.slice(1))
         .join(' ');

    return parts.map(p =>
        LABEL_ABBREVS[p] || p.split('_').map(splitWord).join(' ')
    ).join(' ');
}

