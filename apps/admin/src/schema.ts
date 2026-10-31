// Generated from the running gateway /openapi.json. Do not hand edit.
export type Accepted = {
  "accepted": boolean;
};

export type AccessPoint = {
  "secured": boolean;
  "signal": number;
  "ssid": string;
};

export type ApiErrorBody = {
  "error": ServiceError;
};

export type ApiKeyInfo = {
  "created_at_ms": number;
  "id": string;
  "last_used_ms"?: number | null;
  "name": string;
  "scope": ApiScope;
};

export type ApiScope = "read" | "control";

export type AuthErrorBody = {
  "error": ErrorDetail;
};

export type Capabilities = {
  "active_probing": boolean;
  "hardware_verified": boolean;
  "passive_discovery": boolean;
  "raw_writes": boolean;
  "rmu40_emulation": boolean;
  "writes": boolean;
};

export type Catalog = {
  "catalog_id": string;
  "catalog_version": string;
  "model": string;
  "registers": Array<Entry>;
  "schema": string;
  "schema_version": number;
  "sources": {
  [key: string]: unknown;
};
  "write_policy": string;
  [key: string]: unknown;
};

export type CatalogImport = {
  "catalog_version": string;
  "count": number;
  "writes_enabled": boolean;
};

export type ChangePassword = {
  "current_password": string;
  "new_password": string;
};

export type Command = {
  "kind": "temporary_lux";
  "mode": TemporaryLux;
} | {
  "kind": "heating_offset";
  "offset": number;
} | {
  "kind": "alarm_reset";
};

export type CommandReceipt = {
  "accepted_at_ms": number;
  "command": Command;
  "detail": string;
  "sequence": number;
};

export type ConnectionState = "connecting" | "connected" | "disconnected";

export type ConnectRequest = {
  "password": string;
  "ssid": string;
};

export type ControlLease = {
  "baseline": number;
  "expires_at_ms": number;
  "granted_at_ms": number;
  "holder": string;
  "offset": number;
  "pending_supersession"?: Array<unknown> | null;
};

export type CreateApiKey = {
  "name": string;
  "scope": ApiScope;
};

export type CreatedApiKey = (ApiKeyInfo) & ({
  "token": string;
});

export type DataType = "u8" | "s8" | "u16" | "s16" | "u32" | "s32";

export type DeveloperRequest = {
  "confirmation": string;
  "seconds": number;
};

export type DeveloperStatus = {
  "capture_available": boolean;
  "enabled": boolean;
  "seconds_remaining": number;
};

export type DeviceIdentity = {
  "firmware"?: string | null;
  "model": string;
  "serial"?: string | null;
  "source": string;
  "verification": VerificationState;
};

export type DeviceMode = "simulated" | "replay" | "hardware";

export type DeviceSnapshot = {
  "catalog": Array<RegisterDefinition>;
  "connection": ConnectionState;
  "controls_enabled": boolean;
  "diagnostics": Diagnostics;
  "discovery": DiscoveryStatus;
  "identity"?: null | DeviceIdentity;
  "last_error"?: string | null;
  "last_update_ms"?: number | null;
  "measurements": Array<Measurement>;
  "mode": DeviceMode;
  "network": NetworkState;
  "now_ms": number;
  "registers": Array<RegisterReading>;
  "stale": boolean;
  "stale_after_ms": number;
};

export type Diagnostics = {
  "commands_accepted": number;
  "commands_rejected": number;
  "event_receivers_lagged": number;
  "malformed_frames": number;
  "polls": number;
  "timeouts": number;
  "transport_errors": number;
  "updates": number;
};

export type DiscoveryStatus = {
  "active_probing_supported": boolean;
  "detail": string;
  "observed_count": number;
  "state": string;
};

export type Entry = {
  "access": string;
  "conflicts": Array<unknown>;
  "data_type": DataType;
  "description"?: string | null;
  "flags": Array<string>;
  "id": number;
  "name": string;
  "notes": Array<string>;
  "raw_default"?: number | null;
  "raw_max"?: number | null;
  "raw_min"?: number | null;
  "scale": number;
  "sources": Array<string>;
  "title": string;
  "unit"?: string | null;
  "value_map"?: {
  [key: string]: string;
} | null;
  "verification": string;
  "write_allowed": boolean;
};

export type ErrorCode = "validation" | "controls_disabled" | "forbidden" | "busy" | "timeout" | "disconnected" | "malformed" | "rejected" | "unavailable" | "unsupported";

export type ErrorDetail = {
  "code": string;
  "message": string;
};

export type Explorer = {
  "capabilities": Capabilities;
  "catalog_version": string;
  "diagnostics": WireDiagnostics;
  "items": Array<ExplorerEntry>;
  "last_error"?: string | null;
  "probe": ProbeStatus;
  "word_order": WordOrder;
};

export type ExplorerEntry = {
  "catalog_known": boolean;
  "definition"?: null | Entry;
  "id": number;
  "reading"?: null | RegisterReading;
  "state": RegisterState;
  "verification": string;
  "writable": boolean;
};

export type GatewayEvent = {
  "at_ms": number;
  "detail": string;
  "kind": string;
  "sequence": number;
};

export type Health = {
  "device_degraded": boolean;
  "device_fatal": boolean;
  "last_frame_age_ms"?: number | null;
  "poll_rounds"?: number | null;
  "process_instance": string;
  "ready": boolean;
  "release_id": string;
  "status": string;
  "version": string;
  "wire_priority_applied": boolean;
  "wire_recovering": boolean;
  "wire_reopen_attempts": number;
};

export type HeatingOffsetRequest = {
  "lease_seconds"?: number | null;
  "offset": number;
};

export type HistoryPoint = {
  "last": number;
  "max": number;
  "mean": number;
  "min": number;
  "t": number;
};

export type HistoryResponse = {
  "from": number;
  "series": Array<HistorySeries>;
  "to": number;
};

export type HistorySeries = {
  "bucket_s": number;
  "kind": string;
  "name": string;
  "points": Array<HistoryPoint>;
  "register": number;
  "resolution": string;
  "unit"?: string | null;
};

export type HistoryStatus = {
  "buffered": number;
  "clock_synced": boolean;
  "enabled": boolean;
  "last_error"?: string | null;
  "last_flush_duration_ms"?: number | null;
  "last_flush_ms"?: number | null;
  "samples_dropped_clock": number;
  "samples_written": number;
  "size_bytes": number;
};

export type LeaseRestore = {
  "at_ms": number;
  "baseline": number;
  "detail": string;
  "holder": string;
  "ok": boolean;
};

export type LeaseStatus = {
  "heating_offset"?: null | ControlLease;
  "last_restore"?: null | LeaseRestore;
  "remaining_s"?: number | null;
};

export type Measurement = {
  "age_ms": number;
  "error"?: string | null;
  "id": u32;
  "name": string;
  "observed_at_ms": number;
  "stale": boolean;
  "unit"?: string | null;
  "value"?: null | RegisterValue;
};

export type MqttStatus = {
  "broker"?: string | null;
  "commands_received": number;
  "connected_since_ms"?: number | null;
  "last_error"?: string | null;
  "messages_published": number;
  "state": string;
};

export type MqttUpdate = {
  "allow_commands": boolean;
  "discovery": boolean;
  "discovery_prefix": string;
  "enabled": boolean;
  "host": string;
  "password"?: string | null;
  "port": number;
  "topic_prefix": string;
  "username"?: string | null;
};

export type MqttView = {
  "allow_commands": boolean;
  "available_commands": Array<string>;
  "base_topic": string;
  "discovery": boolean;
  "discovery_prefix": string;
  "enabled": boolean;
  "host": string;
  "password_set": boolean;
  "port": number;
  "status": MqttStatus;
  "topic_prefix": string;
  "username"?: string | null;
};

export type NetworkState = "unmanaged" | "connected" | "setup_required" | "authentication_failed" | "recovering";

export type NetworkStatus = {
  "ip_addresses": Array<string>;
  "last_error"?: string | null;
  "setup_ssid"?: string | null;
  "simulated": boolean;
  "ssid"?: string | null;
  "state": ProvisioningNetworkState;
};

export type NotesMode = "merge" | "replace";

export type ObservationState = "catalog_known" | "observed" | "confirmed_readable" | "unsupported" | "unknown";

export type PasswordRequest = {
  "password": string;
};

export type PersistedExplorer = (Explorer) & ({
  "register_notes": {
  [key: string]: RegisterNote;
};
  "verification_overlay": {
  [key: string]: RegisterVerification;
};
});

export type PollLists = {
  "fast": Array<number>;
  "overrides": {
  [key: string]: PollMode;
};
  "slow": Array<number>;
};

export type PollMode = "fast" | "slow" | "off";

export type PollStatus = {
  "reads": number;
  "registers": Array<number>;
  "rounds_ok"?: number;
  "slow_interval_s"?: number;
  "slow_registers"?: Array<number>;
  "timeouts": number;
};

export type ProbeRequest = {
  "register_ids": Array<number>;
};

export type ProbeStatus = {
  "cancelled": boolean;
  "completed": number;
  "detail": string;
  "outstanding"?: number | null;
  "poll"?: PollStatus;
  "queued": number;
  "running": boolean;
  "states": {
  [key: string]: RegisterState;
};
  "writes"?: WriteStatus;
};

export type ProvisioningNetworkState = "ap" | "connecting" | "connected" | "offline" | "recovery_ap";

export type RegisterAccess = "read_only" | "read_write";

export type RegisterDefinition = {
  "access": RegisterAccess;
  "catalog_version": string;
  "data_type": RegisterType;
  "description": string;
  "firmware"?: string | null;
  "id": u32;
  "models": Array<string>;
  "name": string;
  "scale": number;
  "source": string;
  "unit"?: string | null;
  "verification": VerificationState;
};

export type RegisterList = {
  "items": Array<RegisterReading>;
  "now_ms": number;
  "stale": boolean;
  "stale_after_ms": number;
};

export type RegisterNote = {
  "display_verified"?: boolean | null;
  "label"?: string | null;
  "note"?: string | null;
  "relevance": Relevance;
  "source": string;
  "timestamp_ms": number;
};

export type RegisterNoteInput = {
  "display_verified"?: boolean | null;
  "label"?: string | null;
  "note"?: string | null;
  "register": number;
  "relevance": Relevance;
  "source": string;
};

export type RegisterNotesUpdate = {
  "mode"?: NotesMode;
  "notes": Array<RegisterNoteInput>;
};

export type RegisterNotesView = {
  "register_notes": {
  [key: string]: RegisterNote;
};
};

export type RegisterPollRequest = {
  "mode": PollMode;
  "register": number;
};

export type RegisterReading = {
  "decoded"?: null | RegisterValue;
  "error"?: string | null;
  "id": u32;
  "max_age_ms"?: number | null;
  "observation": ObservationState;
  "observed_at_ms": number;
  "raw"?: null | RegisterValue;
  "source": string;
};

export type RegisterReadReceipt = {
  "reading": RegisterReading;
  "register": number;
};

export type RegisterReadRequest = {
  "register": number;
};

export type RegisterState = "catalog-known" | "observed" | "confirmed-readable" | "unsupported" | "unverified" | "unknown";

export type RegisterType = "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "boolean" | "text";

export type RegisterValue = {
  "type": "integer";
  "value": number;
} | {
  "type": "decimal";
  "value": number;
} | {
  "type": "boolean";
  "value": boolean;
} | {
  "type": "text";
  "value": string;
};

export type RegisterVerification = {
  "source": string;
  "state": string;
  "timestamp_ms": number;
};

export type RegisterWriteReceipt = {
  "detail": string;
  "raw": number;
  "register": number;
};

export type RegisterWriteRequest = {
  "raw"?: number | null;
  "register": number;
  "value"?: number | null;
};

export type Relevance = "relevant" | "absent" | "suspicious" | "unmaintained";

export type RemoteRelease = {
  "asset_size": number;
  "newer": boolean;
  "published_at"?: string | null;
  "url": string;
  "version": string;
};

export type RemoteStatus = {
  "checked_at_ms"?: number | null;
  "current_version": string;
  "downloading": boolean;
  "enabled": boolean;
  "error"?: string | null;
  "latest"?: null | RemoteRelease;
  "repository"?: string | null;
};

export type ResetRequest = {
  "confirmation": string;
  "keep_history"?: boolean;
};

export type ResetResponse = {
  "reset": boolean;
};

export type ScanResponse = {
  "networks": Array<AccessPoint>;
};

export type ServiceError = {
  "code": ErrorCode;
  "message": string;
};

export type SessionStatus = {
  "authenticated": boolean;
  "setup_required": boolean;
};

export type SetupStatus = {
  "network_state": ProvisioningNetworkState;
  "setup_required": boolean;
  "setup_ssid"?: string | null;
  "setup_url": string;
  "simulated": boolean;
};

export type SshKey = {
  "key": string;
};

export type SshKeyFingerprint = {
  "fingerprint_sha256": string;
};

export type SshKeys = {
  "keys": Array<string>;
};

export type StageRequest = {
  "version": string;
};

export type Status = {
  "connection": ConnectionState;
  "controls_enabled": boolean;
  "heating_offset_lease"?: null | LeaseStatus;
  "ip_addresses": Array<string>;
  "last_error"?: string | null;
  "last_update_ms"?: number | null;
  "mode": DeviceMode;
  "network": NetworkState;
  "now_ms": number;
  "stale": boolean;
  "stale_after_ms": number;
  "uptime_s": number;
};

export type SupportBundle = {
  "connection": ConnectionState;
  "controls_enabled": boolean;
  "counters": Diagnostics;
  "format_version": number;
  "gateway_version": string;
  "mode": DeviceMode;
  "register_count": number;
  "stale": boolean;
};

export type TailscaleDisconnect = {
  "logout"?: boolean;
};

export type TailscaleJoin = {
  "auth_key": string;
  "hostname"?: string | null;
};

export type TemporaryLux = "off" | "three_hours" | "six_hours" | "twelve_hours" | "one_time_increase";

export type TemporaryLuxRequest = {
  "mode": TemporaryLux;
};

export type u32 = number;

export type UpdateStatus = {
  "available": boolean;
  "state"?: unknown;
};

export type VerificationState = "unverified" | "capture_verified" | "hardware_verified" | "synthetic";

export type WireDiagnostics = {
  "ack_deadline_misses": number;
  "acks_received": number;
  "acks_sent": number;
  "capture_dropped": number;
  "capture_failed": boolean;
  "checksum_errors": number;
  "frames_for_other_addresses": number;
  "frames_for_us": number;
  "garbage_bytes": number;
  "invalid_escapes": number;
  "malformed_payloads": number;
  "max_reply_us": number;
  "naks_received": number;
  "naks_sent": number;
  "observations_dropped": number;
  "read_requests_sent": number;
  "write_requests_sent"?: number;
};

export type WordOrder = "unknown" | "low_word_first" | "high_word_first";

export type WriteOutcome = {
  "queued_at_ms": number;
  "raw": number;
  "register": number;
  "state": WriteState;
  "updated_at_ms": number;
};

export type WriteState = "queued" | "sent" | "accepted" | "denied" | "timeout";

export type WriteStatus = {
  "accepted": number;
  "alarm_auto_reset"?: boolean;
  "alarm_reset"?: boolean;
  "allowlist": Array<number>;
  "auto_resets"?: number;
  "denied": number;
  "enabled": boolean;
  "last"?: null | WriteOutcome;
  "open"?: boolean;
  "sent": number;
  "timeouts": number;
};
