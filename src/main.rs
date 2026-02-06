/**
 * N1 Encoder Actions - OpenDeck Plugin
 * 
 * Provides configurable multi-action support for the Ajazz N1 encoder dial.
 * Uses mirajazz for device communication, same as the main opendeck-ajazz-n1 plugin.
 * 
 * Input Mapping (from N1 device):
 * - INPUT 50 (0x32): Rotate Counter-Clockwise (-1)
 * - INPUT 51 (0x33): Rotate Clockwise (+1)
 * - INPUT 35 (0x23): Dial Press (state 1 = pressed, state 0 = released)
 */

use futures_lite::StreamExt;
use mirajazz::{
    device::{Device, DeviceWatcher, list_devices, DeviceQuery},
    error::MirajazzError,
    state::DeviceStateUpdate,
    types::{DeviceInput, DeviceLifecycleEvent, HidDeviceInfo},
};
use openaction::async_trait;
use openaction::global_events::{
    GlobalEventHandler, SetBrightnessEvent, SetImageEvent,
};
use openaction::OpenActionResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

// N1 Device identification
const N1_VID: u16 = 0x0300;
const N1_PID: u16 = 0x3007;
const DEVICE_NAMESPACE: &str = "N1";

// N1 layout constants
const N1_ROWS: usize = 6;
const N1_COLS: usize = 3;
const N1_KEY_COUNT: usize = 18;  // 15 buttons + 3 top LCDs
const N1_ENCODER_COUNT: usize = 3;  // 2 face buttons + 1 dial

// Input IDs for N1 encoder (based on device protocol)
const INPUT_DIAL_CCW: u8 = 50;      // Rotate counter-clockwise (-1)
const INPUT_DIAL_CW: u8 = 51;       // Rotate clockwise (+1)
const INPUT_DIAL_PRESS: u8 = 35;    // Dial press

/// Action mode - what the encoder does when rotated
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ActionMode {
    Volume,
    MediaTrack,
    MediaSeek,
    Scroll,
    Brightness,
    Custom,
}

impl Default for ActionMode {
    fn default() -> Self {
        ActionMode::Volume
    }
}

/// Settings stored per action instance
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ActionSettings {
    #[serde(default)]
    mode: ActionMode,
    #[serde(default)]
    cw_command: String,
    #[serde(default)]
    ccw_command: String,
}

impl Default for ActionSettings {
    fn default() -> Self {
        Self {
            mode: ActionMode::Volume,
            cw_command: String::new(),
            ccw_command: String::new(),
        }
    }
}

/// Global plugin state
#[derive(Default)]
struct PluginState {
    /// Connected devices (device_id -> ())
    devices: RwLock<HashMap<String, ()>>,
    /// Cancellation tokens for device tasks
    tokens: RwLock<HashMap<String, CancellationToken>>,
}

impl PluginState {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

// ============================================================================
// Global Handler
// ============================================================================

struct N1EncoderGlobalHandler {
    state: Arc<PluginState>,
}

impl N1EncoderGlobalHandler {
    fn new(state: Arc<PluginState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl GlobalEventHandler for N1EncoderGlobalHandler {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        log::info!("========================================");
        log::info!("N1 Encoder Actions plugin initialized");
        log::info!("========================================");
        log::info!("Device: Ajazz N1 (VID:{:04X} PID:{:04X})", N1_VID, N1_PID);
        log::info!("Inputs: INPUT50=CCW(-1), INPUT51=CW(+1), INPUT35=Press");
        
        // Start device watcher
        let state = self.state.clone();
        tokio::spawn(watcher_task(state));
        
        Ok(())
    }

    async fn device_plugin_set_image(&self, _event: SetImageEvent) -> OpenActionResult<()> {
        Ok(())
    }

    async fn device_plugin_set_brightness(&self, _event: SetBrightnessEvent) -> OpenActionResult<()> {
        Ok(())
    }
}

// ============================================================================
// Device Watcher
// ============================================================================

/// Device query for N1
const N1_QUERY: DeviceQuery = DeviceQuery::new(65440, 1, N1_VID, N1_PID);

/// Get device ID from HID info
fn get_device_id(dev: &HidDeviceInfo) -> Option<String> {
    if dev.vendor_id == N1_VID && dev.product_id == N1_PID {
        // N1 uses protocol v3 with unique serial
        Some(format!("{}-{}", DEVICE_NAMESPACE, dev.serial_number.clone()?))
    } else {
        None
    }
}

/// Watch for N1 device connection/disconnection
async fn watcher_task(state: Arc<PluginState>) {
    let mut watcher = DeviceWatcher::new();
    let queries = [N1_QUERY];
    
    let mut stream = match watcher.watch(&queries).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to start device watcher: {}", e);
            return;
        }
    };
    
    log::info!("Device watcher started, looking for N1...");
    
    // Check for already-connected devices
    scan_and_connect_devices(&state).await;
    
    // Watch for new connections
    loop {
        match stream.next().await {
            Some(DeviceLifecycleEvent::Connected(_info)) => {
                // New device connected, scan for it
                scan_and_connect_devices(&state).await;
            }
            Some(DeviceLifecycleEvent::Disconnected(info)) => {
                if let Some(id) = get_device_id(&info) {
                    log::info!("N1 disconnected: {}", id);
                    if let Some(token) = state.tokens.write().await.remove(&id) {
                        token.cancel();
                    }
                    state.devices.write().await.remove(&id);
                    let _ = openaction::device_plugin::unregister_device(id).await;
                }
            }
            None => {
                log::info!("Device watcher stream ended");
                break;
            }
        }
    }
}

/// Scan for devices and connect to new ones
async fn scan_and_connect_devices(state: &Arc<PluginState>) {
    let queries = [N1_QUERY];
    
    match list_devices(&queries).await {
        Ok(devices) => {
            for dev in devices {
                // Get device info (consumes dev)
                let dev_info = dev.to_device_info();
                
                if let Some(id) = get_device_id(&dev_info) {
                    if state.devices.read().await.contains_key(&id) {
                        continue;
                    }
                    
                    log::info!("N1 found: {}", id);
                    let state_clone = state.clone();
                    let token = CancellationToken::new();
                    state.tokens.write().await.insert(id.clone(), token.clone());
                    tokio::spawn(handle_device(state_clone, dev_info, id, token));
                }
            }
        }
        Err(e) => log::error!("Failed to list devices: {}", e),
    }
}

// ============================================================================
// Device Handler
// ============================================================================

/// Handle a connected N1 device
async fn handle_device(
    state: Arc<PluginState>,
    dev_info: HidDeviceInfo,
    device_id: String,
    token: CancellationToken,
) {
    log::info!("Connecting to N1: {}", device_id);
    
    // Connect to device using mirajazz
    let device = match Device::connect(
        &dev_info,
        3,  // protocol version
        N1_KEY_COUNT,
        N1_ENCODER_COUNT,
    ).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to connect to {}: {}", device_id, e);
            state.tokens.write().await.remove(&device_id);
            return;
        }
    };
    
    // Set software mode and init
    if let Err(e) = device.set_mode(3).await {
        log::error!("Failed to set mode: {}", e);
        state.tokens.write().await.remove(&device_id);
        return;
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    if let Err(e) = device.set_brightness(50).await {
        log::error!("Failed to set brightness: {}", e);
    }
    device.clear_all_button_images().await.ok();
    device.flush().await.ok();
    
    // Register with OpenDeck
    if let Err(e) = openaction::device_plugin::register_device(
        device_id.clone(),
        "Ajazz N1".to_string(),
        N1_ROWS as u8,
        N1_COLS as u8,
        N1_ENCODER_COUNT as u8,
        0,
    ).await {
        log::error!("Failed to register device: {}", e);
        state.tokens.write().await.remove(&device_id);
        return;
    }
    
    log::info!("N1 registered: {} ({} encoders)", device_id, N1_ENCODER_COUNT);
    
    // Mark device as connected
    state.devices.write().await.insert(device_id.clone(), ());
    
    // Create input reader
    let reader = device.get_reader(process_input_n1);
    
    log::info!("N1 ready for input: {}", device_id);
    
    // Process events
    loop {
        tokio::select! {
            result = reader.read(None) => {
                match result {
                    Ok(updates) => {
                        for update in updates {
                            handle_device_update(&device_id, &update).await;
                        }
                    }
                    Err(e) => {
                        log::error!("Read error on {}: {}", device_id, e);
                        break;
                    }
                }
            }
            _ = token.cancelled() => {
                log::info!("Device task cancelled: {}", device_id);
                break;
            }
        }
    }
    
    // Cleanup
    log::info!("Disconnecting N1: {}", device_id);
    device.shutdown().await.ok();
    state.devices.write().await.remove(&device_id);
    state.tokens.write().await.remove(&device_id);
    let _ = openaction::device_plugin::unregister_device(device_id).await;
}

/// Process raw input from N1 device
/// Maps INPUT50/51/35 to encoder events
fn process_input_n1(input: u8, input_state: u8) -> Result<DeviceInput, MirajazzError> {
    match input {
        INPUT_DIAL_CCW => Ok(DeviceInput::EncoderTwist(vec![0, 0, -1])),  // CCW
        INPUT_DIAL_CW => Ok(DeviceInput::EncoderTwist(vec![0, 0, 1])),   // CW
        INPUT_DIAL_PRESS => {
            let pressed = input_state != 0;
            Ok(DeviceInput::EncoderStateChange(vec![false, false, pressed]))
        }
        _ => Err(MirajazzError::BadData),
    }
}

/// Handle device state update
async fn handle_device_update(
    device_id: &str,
    update: &DeviceStateUpdate,
) {
    match update {
        // Encoder 2 (the dial) twist
        DeviceStateUpdate::EncoderTwist(2, val) => {
            log::debug!("Dial twist: {} (direction: {})", device_id, val);
            // direction: 1 = CW (INPUT51), -1 = CCW (INPUT50)
            execute_rotation(*val as i8).await;
        }
        
        // Encoder 2 press
        DeviceStateUpdate::EncoderDown(2) => {
            log::info!("Dial pressed: {}", device_id);
            if let Err(e) = openaction::device_plugin::encoder_down(device_id.to_string(), 2).await {
                log::error!("Failed to send encoder_down: {}", e);
            }
        }
        
        DeviceStateUpdate::EncoderUp(2) => {
            log::info!("Dial released: {}", device_id);
            if let Err(e) = openaction::device_plugin::encoder_up(device_id.to_string(), 2).await {
                log::error!("Failed to send encoder_up: {}", e);
            }
        }
        
        // Other encoders (face buttons 0, 1) - forward to OpenDeck
        DeviceStateUpdate::EncoderDown(enc) => {
            let _ = openaction::device_plugin::encoder_down(device_id.to_string(), *enc).await;
        }
        DeviceStateUpdate::EncoderUp(enc) => {
            let _ = openaction::device_plugin::encoder_up(device_id.to_string(), *enc).await;
        }
        DeviceStateUpdate::EncoderTwist(enc, val) => {
            let _ = openaction::device_plugin::encoder_change(
                device_id.to_string(), *enc, *val as i16
            ).await;
        }
        
        // Button events - forward to OpenDeck
        DeviceStateUpdate::ButtonDown(key) => {
            let _ = openaction::device_plugin::key_down(device_id.to_string(), *key).await;
        }
        DeviceStateUpdate::ButtonUp(key) => {
            let _ = openaction::device_plugin::key_up(device_id.to_string(), *key).await;
        }
    }
}

// ============================================================================
// Action Execution
// ============================================================================

/// Execute rotation action based on direction
/// direction: 1 = CW (INPUT51), -1 = CCW (INPUT50)
async fn execute_rotation(direction: i8) {
    // For now, use default settings
    // In a full implementation, this would look up per-action settings
    let settings = ActionSettings::default();
    
    log::debug!("Executing rotation: direction={}, mode={:?}", direction, settings.mode);
    
    let result = match settings.mode {
        ActionMode::Volume => execute_volume(direction),
        ActionMode::MediaTrack => execute_media_track(direction),
        ActionMode::MediaSeek => execute_media_seek(direction),
        ActionMode::Scroll => execute_scroll(direction),
        ActionMode::Brightness => execute_brightness(direction),
        ActionMode::Custom => execute_custom(direction, &settings),
    };
    
    if let Err(e) = result {
        log::error!("Action failed: {}", e);
    }
}

// ============================================================================
// Command Execution
// ============================================================================

fn execute_volume(direction: i8) -> Result<(), Box<dyn std::error::Error>> {
    let sign = if direction > 0 { "+" } else { "-" };
    let cmd = format!("amixer sset Master 5%{}", sign);
    log::info!("Volume: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_media_track(direction: i8) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = if direction > 0 { "playerctl next" } else { "playerctl previous" };
    log::info!("Media: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(cmd).output()?;
    if !output.status.success() {
        log::debug!("playerctl: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn execute_media_seek(direction: i8) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = if direction > 0 { "playerctl position 5+" } else { "playerctl position 5-" };
    log::info!("Seek: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(cmd).output()?;
    if !output.status.success() {
        log::debug!("playerctl seek: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn execute_scroll(direction: i8) -> Result<(), Box<dyn std::error::Error>> {
    let button = if direction > 0 { 5 } else { 4 }; // 5=down, 4=up
    let cmd = format!("xdotool click --repeat 3 {}", button);
    log::info!("Scroll: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_brightness(direction: i8) -> Result<(), Box<dyn std::error::Error>> {
    let sign = if direction > 0 { "+" } else { "-" };
    let cmd = format!("brightnessctl set 10%{}", sign);
    log::info!("Brightness: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_custom(direction: i8, settings: &ActionSettings) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = if direction > 0 { &settings.cw_command } else { &settings.ccw_command };
    if cmd.is_empty() {
        return Ok(());
    }
    log::info!("Custom: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )?;
    
    let state = PluginState::new();
    
    static HANDLER: std::sync::OnceLock<N1EncoderGlobalHandler> = std::sync::OnceLock::new();
    HANDLER.set(N1EncoderGlobalHandler::new(state)).ok();
    openaction::global_events::set_global_event_handler(HANDLER.get().unwrap());
    
    openaction::run(std::env::args().collect()).await?;
    
    log::info!("Plugin shutting down");
    Ok(())
}
