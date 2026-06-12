export interface AppConfig {
  serverUrl: string;
  clientId: string;
  passkey: string;
  acpUrl?: string | null;
  storagePath?: string | null;
  acpCwd?: string | null;
  floatImage?: string | null;
  allowedReadPaths?: string[];
  allowedWritePaths?: string[];
  deniedExtensions?: string[];
  toolPermissions?: Record<string, boolean>;
}

export interface FileTransferRecord {
  id: string;
  file_name: string;
  file_size: number;
  direction: string;
  status: string;
  file_path: string | null;
  kaya_session_id: string | null;
  acp_session_id: string | null;
  created_at: string;
}

export interface TransferRecord {
  id: string;
  name: string;
  size: number;
  direction: "received" | "sent";
  timestamp: number;
  status: "ok" | "error";
  path?: string;
}

export interface KayaSessionRecord {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  is_active: boolean;
}

export interface ChatMessageRecord {
  id: string;
  kaya_session_id: string;
  acp_session_id: string | null;
  role: string;
  content: string;
  created_at: string;
}
