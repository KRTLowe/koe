use serde_json::Value;
use std::process::Command;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct SystemInfoTool {
    enabled: bool,
}

impl SystemInfoTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("system_info")
                .copied()
                .unwrap_or(true),
        }
    }
}

/// PowerShell 一行拿齐所有系统信息，输出 JSON。
/// Get-CimInstance 不需要管理员权限。
const PS_SCRIPT: &str = r#"
$os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,FreePhysicalMemory,TotalVisibleMemorySize,LastBootUpTime;
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1 Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed;
$cs = Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory,Manufacturer,Model;
$gpu = Get-CimInstance Win32_VideoController | Where-Object {$_.AdapterDACType -ne 'Internal'} | Select-Object -First 1 Name,AdapterRAM,DriverVersion;
$disks = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,@{N='SizeGB';E={[math]::Round($_.Size/1GB,1)}},@{N='FreeGB';E={[math]::Round($_.FreeSpace/1GB,1)}};
$net = Get-CimInstance Win32_NetworkAdapter | Where-Object {$_.NetEnabled -eq $true -and $_.AdapterType -match 'Ethernet|Wireless'} | Select-Object Name,AdapterType,Speed;

@{
    OS = "$($os.Caption) $($os.Version)"
    Uptime = "$([math]::Round(((Get-Date) - $os.LastBootUpTime).TotalHours,1)) hours"
    CPU = "$($cpu.Name) ($($cpu.NumberOfCores)C/$($cpu.NumberOfLogicalProcessors)T @ $($cpu.MaxClockSpeed)MHz)"
    Memory = "Total: $([math]::Round($cs.TotalPhysicalMemory/1GB,0))GB  Free: $([math]::Round($os.FreePhysicalMemory/1MB,0))MB"
    GPU = if ($gpu) { "$($gpu.Name) ($([math]::Round($gpu.AdapterRAM/1GB,1))GB)" } else { "N/A" }
    Disks = ($disks | ForEach-Object { "$($_.DeviceID) $($_.SizeGB)GB ($($_.FreeGB)GB free)" }) -join ' | '
    Network = ($net | ForEach-Object { "$($_.Name) ($($_.AdapterType) $($_.Speed)bps)" }) -join ' | '
} | ConvertTo-Json -Compress
"#;

impl Tool for SystemInfoTool {
    fn name(&self) -> &'static str {
        "system_info"
    }

    fn description(&self) -> &'static str {
        "Get system information: OS, CPU, memory, GPU, disk space, network adapters"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, _args: &Value) -> ToolResult {
        log::info!("[SystemInfo] collecting...");

        let output = match Command::new("powershell")
            .args(["-NoProfile", "-Command", PS_SCRIPT])
            .output()
        {
            Ok(o) => o,
            Err(e) => return ToolResult::err(format!("Failed to run PowerShell: {}", e)),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stderr.trim().is_empty() {
            log::warn!("[SystemInfo] PowerShell stderr: {}", stderr.trim());
        }

        if stdout.is_empty() {
            return ToolResult::err("No system information returned".to_string());
        }

        // 尝试解析 JSON 并格式化
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(info) => {
                let text = format!(
                    "OS:      {}\nUptime:  {}\nCPU:     {}\nMemory:  {}\nGPU:     {}\nDisks:   {}\nNetwork: {}",
                    info["OS"].as_str().unwrap_or("?"),
                    info["Uptime"].as_str().unwrap_or("?"),
                    info["CPU"].as_str().unwrap_or("?"),
                    info["Memory"].as_str().unwrap_or("?"),
                    info["GPU"].as_str().unwrap_or("?"),
                    info["Disks"].as_str().unwrap_or("?"),
                    info["Network"].as_str().unwrap_or("?"),
                );
                log::info!("[SystemInfo] done");
                ToolResult::ok(text)
            }
            Err(_) => {
                // JSON 解析失败，返回原始输出
                log::info!("[SystemInfo] JSON parse failed, returning raw");
                ToolResult::ok(stdout)
            }
        }
    }
}
