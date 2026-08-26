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
};

export type Device = {
  device_id: string;
  connected: boolean;
  last_signal?: number;
  sensor_id?: string;
  first_seen?: number;
  last_seen?: number;
  event_count?: number;
  last_type?: string;
  hostname?: string;
  band?: string;
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
  const data = await fetchJson<{ networks?: Network[] }>(
    `${API}/reports/networks?hours=24`
  );
  return data.networks || [];
}

export async function fetchStats(): Promise<Stats> {
  return fetchJson<Stats>(`${API}/stats`);
}

export async function fetchAllDevices(): Promise<DetailedDevice[]> {
  const data = await fetchJson<{ devices?: DetailedDevice[] }>(
    `${API}/devices?limit=200`
  );
  return data.devices || [];
}

export async function fetchTimeline(): Promise<Timeline> {
  return fetchJson<Timeline>(`${API}/timeline?hours=24`);
}
