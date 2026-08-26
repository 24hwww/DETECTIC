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
};

export type Network = {
  ap_id: string;
  ssid?: string;
  status?: string;
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
