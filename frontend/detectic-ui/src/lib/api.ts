const API = "/api/v1";

export type Sensor = {
  id: string;
  name?: string;
  location?: {
    latitude?: number;
    longitude?: number;
    source?: string;
    accuracy_m?: number;
  } | null;
  public_ip?: string;
  ap_count?: number;
  distinct_devices?: number;
  total_devices?: number;
  last_seen?: number;
  created_at?: number | null;
};

export type Device = {
  device_id: string;
  connected: boolean;
  state?: string;
  last_signal?: number;
  sensor_id?: string;
  first_seen?: number;
  last_seen?: number;
  event_count?: number;
  last_type?: string;
  hostname?: string;
  band?: string;
  proximity?: string | null;
};

export type DetailedDevice = {
  pseudonym: string;
  manufacturer?: string | null;
  brand?: string | null;
  model_guess?: string | null;
  device_class?: string | null;
  mac_type?: string | null;
  confidence?: number | null;
  confidence_label?: string | null;
  bssid_manufacturer?: string | null;
  hostname?: string | null;
  band?: string | null;
  operating_standard?: string | null;
  status?: string | null;
  bssid_pseudonym?: string | null;
  signal_strength?: number | null;
  avg_rssi?: number | null;
  observations?: number;
  first_seen?: number | null;
  last_seen?: number | null;
  fingerprint_model?: string | null;
  fingerprint_confidence?: number | null;
  alias?: string | null;
  owner?: string | null;
  room?: string | null;
  tags?: string | null;
  notes?: string | null;
};

export type Network = {
  ap_id: string;
  ssid?: string;
  status?: string;
  sensor_id?: string;
  first_seen?: number;
  last_seen?: number;
  event_count?: number;
  band?: string;
  w_mode?: string;
  security?: string;
  last_signal?: number;
  online_since?: number;
  channel?: number;
  current_signal?: number;
  average_signal?: number;
  min_signal?: number;
  max_signal?: number;
  rssi_variance?: number;
  observation_count?: number;
  session_count?: number;
  extch?: string;
  proximity?: string | null;
  proximity_detail?: string | Record<string, unknown> | null;
};

export type RfSnapshot = {
  event_id: string;
  sensor_id?: string;
  event_timestamp: number;
  ap_count?: number;
  ap_count_2_4?: number;
  ap_count_5?: number;
  strongest_signal?: number;
  weakest_signal?: number;
  average_signal?: number;
  rssi_variance?: number;
  channel_distribution?: string | Record<string, number>;
  top_aps?: string | unknown[];
};

export type NetworksResponse = {
  hours: number;
  sensor_id: string | null;
  aps: Network[];
  rf_snapshots: RfSnapshot[];
};

export type Stats = {
  distinct_devices?: number;
  total_detections?: number;
  total_snapshots?: number;
  snapshots_last_hour?: number;
  snapshots_last_day?: number;
  total_sensors?: number;
  randomized_macs?: number;
  identified_devices?: number;
  known_vendors?: number;
  avg_rssi?: number;
  total_networks?: number;
};

export type TimelinePoint = {
  pseudonym: string;
  rssi?: number;
  band?: string;
  bssid_pseudonym?: string;
  ts: number;
};

export type Timeline = {
  hours: number;
  points: TimelinePoint[];
};

export type AnalyticsBucket = {
  bucket: string;
  connected?: number;
  disconnected?: number;
  nearby?: number;
  avg?: number | null;
  min?: number | null;
  max?: number | null;
  immediate?: number;
  near?: number;
  medium?: number;
  far?: number;
  unknown?: number;
};

export type ActivityHour = { hour: number; count: number };

export type Dweller = {
  device_id: string;
  manufacturer?: string | null;
  device_class?: string | null;
  total_seconds: number;
  total_minutes: number;
  sessions: number;
  last_signal?: number | null;
};

export type AnalyticsTotals = {
  total_connected: number;
  total_disconnected: number;
  total_observed: number;
  total_nearby_events: number;
  avg_session_seconds: number;
  total_dwell_hours: number;
  peak_hour: number | null;
  peak_hour_connections: number;
};

export type Analytics = {
  hours: number;
  granularity: "hour" | "day";
  cutoff: number;
  connectionTimeline: { bucket: string; connected: number }[];
  disconnectionTimeline: { bucket: string; disconnected: number }[];
  nearbyTimeline: { bucket: string; nearby: number }[];
  rssiTimeline: { bucket: string; avg: number | null; min: number | null; max: number | null }[];
  proximityTimeline: {
    bucket: string;
    immediate: number;
    near: number;
    medium: number;
    far: number;
    unknown: number;
  }[];
  activityByHour: ActivityHour[];
  topDwellers: Dweller[];
  totals: AnalyticsTotals;
};

export type SystemEvent = {
  id?: number;
  event_id: string;
  sensor_id?: string;
  event_type: string;
  event_timestamp: number;
  device_id?: string;
  payload_json?: string;
  sequence?: number;
  received_at?: number;
};

export type Health = {
  status: string;
  timestamp?: number;
};

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} en ${url}`);
  return res.json();
}

export async function fetchSensors(): Promise<Sensor[]> {
  const data = await fetchJson<{ sensors?: Sensor[] }>(`${API}/sensors`);
  return data.sensors || [];
}

export async function fetchDevices(): Promise<Device[]> {
  const data = await fetchJson<{ devices?: Device[] }>(
    `${API}/reports/devices?hours=24`
  );
  return data.devices || [];
}

export async function fetchNetworks(): Promise<Network[]> {
  const data = await fetchJson<{ aps?: Network[]; networks?: Network[] }>(
    `${API}/networks?hours=168`
  );
  return data.aps || data.networks || [];
}

export async function fetchRecentNetworks(): Promise<Network[]> {
  const data = await fetchJson<{ aps?: Network[]; networks?: Network[] }>(
    `${API}/networks?hours=24`
  );
  return data.aps || data.networks || [];
}

export async function fetchAllNetworks(): Promise<NetworksResponse> {
  return fetchJson<NetworksResponse>(`${API}/networks?hours=168`);
}

export async function fetchStats(): Promise<Stats> {
  return fetchJson<Stats>(`${API}/stats`);
}

export type DeviceIdentity = {
  pseudonym: string;
  sensor_id?: string | null;
  manufacturer?: string | null;
  brand?: string | null;
  model_guess?: string | null;
  device_class?: string | null;
  mac_type?: string | null;
  confidence?: number | null;
  confidence_label?: string | null;
  bssid_manufacturer?: string | null;
  identity_json?: string | null;
  fingerprint_id?: string | null;
  last_seen?: number | null;
  alias?: string | null;
  owner?: string | null;
  room?: string | null;
  tags?: string | null;
  notes?: string | null;
  updated_at?: number | null;
};

export async function fetchAllDevices(): Promise<DetailedDevice[]> {
  const data = await fetchJson<{ devices?: DetailedDevice[] }>(
    `${API}/devices?limit=200`
  );
  return data.devices || [];
}

export async function fetchDeviceIdentity(
  deviceId: string
): Promise<DeviceIdentity | null> {
  const data = await fetchJson<{ identity?: DeviceIdentity }>(
    `${API}/devices/${encodeURIComponent(deviceId)}/identity`
  );
  return data.identity || null;
}

export async function updateDeviceIdentity(
  deviceId: string,
  identity: Partial<Pick<DeviceIdentity, "alias" | "owner" | "room" | "tags" | "notes">>
): Promise<DeviceIdentity> {
  const res = await fetch(
    `${API}/devices/${encodeURIComponent(deviceId)}/identity`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(identity),
    }
  );
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const data = await res.json() as { identity: DeviceIdentity };
  return data.identity;
}

export async function fetchTimeline(): Promise<Timeline> {
  return fetchJson<Timeline>(`${API}/timeline?hours=24`);
}

export async function fetchAnalytics(hours = 24): Promise<Analytics> {
  return fetchJson<Analytics>(`${API}/analytics?hours=${hours}`);
}

export async function fetchEvents(
  hours = 168,
  limit = 300
): Promise<SystemEvent[]> {
  const data = await fetchJson<{ events?: SystemEvent[] }>(
    `${API}/events?hours=${hours}&limit=${limit}`
  );
  return data.events || [];
}

export async function fetchHealth(): Promise<Health | null> {
  try {
    return await fetchJson<Health>(`${API}/healthz`);
  } catch {
    return null;
  }
}
