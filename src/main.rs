/**
 * N1 Encoder Actions - OpenDeck Plugin
 * 
 * Provides configurable multi-action support for the Ajazz N1 encoder dial.
 */

use openaction::{
    Action, Instance, OpenActionResult,
    async_trait,
    global_events::GlobalEventHandler,
};
use serde::{Deserialize, Serialize};
use std::process::Command;

// Action UUIDs from manifest.json
const ACTION_ROTATE_UUID: &str = "net.ashurtech.n1-encoder-actions.rotate";
const ACTION_PRESS_UUID: &str = "net.ashurtech.n1-encoder-actions.press";

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

/// Settings for rotate action
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RotateSettings {
    #[serde(default)]
    mode: ActionMode,
    #[serde(default)]
    cw_command: String,
    #[serde(default)]
    ccw_command: String,
}

impl Default for RotateSettings {
    fn default() -> Self {
        Self {
            mode: ActionMode::Volume,
            cw_command: String::new(),
            ccw_command: String::new(),
        }
    }
}

/// Settings for press action (no special settings needed)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct PressSettings {}

// ============================================================================
// Global Handler
// ============================================================================

struct N1EncoderGlobalHandler;

#[async_trait]
impl GlobalEventHandler for N1EncoderGlobalHandler {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        log::info!("N1 Encoder Actions plugin initialized");
        Ok(())
    }
}

// ============================================================================
// Rotate Action - Handles encoder rotation
// ============================================================================

struct RotateAction;

#[async_trait]
impl Action for RotateAction {
    const UUID: &'static str = ACTION_ROTATE_UUID;
    type Settings = RotateSettings;

    async fn will_appear(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Rotate action appeared: {} (mode: {:?})", instance.instance_id, settings.mode);
        Ok(())
    }

    async fn will_disappear(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Rotate action disappeared: {}", instance.instance_id);
        Ok(())
    }

    /// Called when encoder is rotated
    /// ticks: positive for CW, negative for CCW
    /// pressed: true if dial is being pressed while rotating
    async fn dial_rotate(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
        ticks: i16,
        pressed: bool,
    ) -> OpenActionResult<()> {
        let direction = if ticks > 0 { 1 } else { -1 };
        log::info!(
            "Dial rotate: {} (ticks: {}, pressed: {}, mode: {:?})",
            instance.instance_id, ticks, pressed, settings.mode
        );

        if let Err(e) = execute_rotation(direction, settings) {
            log::error!("Rotation action failed: {}", e);
            let _ = instance.show_alert().await;
        } else {
            let _ = instance.show_ok().await;
        }

        Ok(())
    }

    async fn did_receive_settings(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Rotate settings updated: {} (mode: {:?})", instance.instance_id, settings.mode);
        Ok(())
    }
}

// ============================================================================
// Press Action - Handles encoder press
// ============================================================================

struct PressAction;

#[async_trait]
impl Action for PressAction {
    const UUID: &'static str = ACTION_PRESS_UUID;
    type Settings = PressSettings;

    async fn will_appear(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Press action appeared: {}", instance.instance_id);
        Ok(())
    }

    async fn will_disappear(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Press action disappeared: {}", instance.instance_id);
        Ok(())
    }

    /// Called when encoder is pressed down
    async fn dial_down(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Dial pressed: {}", instance.instance_id);
        // The press action just triggers - the actual behavior is defined by multi-actions
        Ok(())
    }

    /// Called when encoder is released
    async fn dial_up(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        log::info!("Dial released: {}", instance.instance_id);
        Ok(())
    }
}

// ============================================================================
// Command Execution
// ============================================================================

fn execute_rotation(direction: i8, settings: &RotateSettings) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match settings.mode {
        ActionMode::Volume => execute_volume(direction),
        ActionMode::MediaTrack => execute_media_track(direction),
        ActionMode::MediaSeek => execute_media_seek(direction),
        ActionMode::Scroll => execute_scroll(direction),
        ActionMode::Brightness => execute_brightness(direction),
        ActionMode::Custom => execute_custom(direction, settings),
    }
}

fn execute_volume(direction: i8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sign = if direction > 0 { "+" } else { "-" };
    let cmd = format!("amixer sset Master 5%{}", sign);
    log::info!("Volume: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_media_track(direction: i8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cmd = if direction > 0 { "playerctl next" } else { "playerctl previous" };
    log::info!("Media: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(cmd).output()?;
    if !output.status.success() {
        log::debug!("playerctl: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn execute_media_seek(direction: i8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cmd = if direction > 0 { "playerctl position 5+" } else { "playerctl position 5-" };
    log::info!("Seek: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(cmd).output()?;
    if !output.status.success() {
        log::debug!("playerctl seek: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn execute_scroll(direction: i8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let button = if direction > 0 { 5 } else { 4 }; // 5=down, 4=up
    let cmd = format!("xdotool click --repeat 3 {}", button);
    log::info!("Scroll: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_brightness(direction: i8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sign = if direction > 0 { "+" } else { "-" };
    let cmd = format!("brightnessctl set 10%{}", sign);
    log::info!("Brightness: {}", cmd);
    
    let output = Command::new("sh").arg("-c").arg(&cmd).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    Ok(())
}

fn execute_custom(direction: i8, settings: &RotateSettings) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    
    log::info!("========================================");
    log::info!("N1 Encoder Actions plugin starting...");
    log::info!("========================================");
    
    // Register global handler (needs to be static)
    static GLOBAL_HANDLER: N1EncoderGlobalHandler = N1EncoderGlobalHandler;
    openaction::global_events::set_global_event_handler(&GLOBAL_HANDLER);
    
    // Register actions
    openaction::register_action(RotateAction).await;
    openaction::register_action(PressAction).await;
    
    // Run the plugin
    openaction::run(std::env::args().collect()).await?;
    
    log::info!("Plugin shutting down");
    Ok(())
}
