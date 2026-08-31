import { parseProximityDetail } from "@/lib/utils";
import type { Device, DetailedDevice, Network } from "@/lib/api";

/**
 * Human-friendly, non-technical labels for the Detectic dashboard.
 *
 * The goal is that anyone looking at the screen can immediately answer:
 *   - who is connected?
 *   - who is closest to the router?
 *   - when was an AP / device first seen?
 *   - how long has it been connected / detected?
 *   - how far away (approximately) is each AP?
 *
 * Raw MAC pseudonyms are NEVER shown. Names come from hostname, manufacturer,
 * model and device type; proximity/signal are words (not dBm), and dates and
 * durations are written out.
 */

const PROXIMITY_WORD: Record<string, string> = {
  immediate: "muy cerca",
  near: "cerca",
  medium: "a cierta distancia",
  far: "lejos",
  edge: "en el límite",
  unknown: "desconocido",
};

const BAND_LABEL: Record<string, string> = {
  "2.4": "2.4 GHz",
  "2.4g": "2.4 GHz",
  "2_4": "2.4 GHz",
  "2.4ghz": "2.4 GHz",
  "5": "5 GHz",
  "5g": "5 GHz",
  "6": "6 GHz",
  "6g": "6 GHz",
  "2g": "2.4 GHz",
  "2.4g/max": "2.4 GHz",
};

/** Friendly band description, e.g. "2.4 GHz". */
export function bandLabel(band?: string | null): string {
  if (!band) return "";
  return BAND_LABEL[band.toLowerCase()] || band;
}

/** Proximity zone as a plain word. */
export function proximityWord(zone?: string | null): string {
  if (!zone) return "desconocido";
  return PROXIMITY_WORD[zone.toLowerCase()] ?? zone.toLowerCase();
}

/** Approximate distance (meters) encoded in the proximity detail, if any. */
export function proximityDistance(
  detail?: string | Record<string, unknown> | null
): number | undefined {
  return parseProximityDetail(detail).distance;
}

/** Proximity zone extracted from proximity or its detail. */
function proximityZoneOf(
  proximity?: string | null,
  detail?: string | Record<string, unknown> | null
): string | undefined {
  if (proximity && proximity.toLowerCase() !== "unknown") return proximity;
  return parseProximityDetail(detail).zone;
}

/** Full proximity phrase, e.g. "muy cerca (~3 m)" or "desconocido". */
export function proximityText(
  proximity?: string | null,
  detail?: string | Record<string, unknown> | null
): string {
  const zone = proximityZoneOf(proximity, detail);
  const word = proximityWord(zone ?? proximity);
  const distance = proximityDistance(detail);
  if (distance != null) return `${word} (~${Math.round(distance)} m)`;
  return word;
}

/** Signal strength -> plain word (0-4). */
export function signalLevel(rssi?: number | null): number {
  if (rssi == null) return 0;
  if (rssi >= -50) return 4;
  if (rssi >= -60) return 3;
  if (rssi >= -70) return 2;
  if (rssi >= -80) return 1;
  return 0;
}

const SIGNAL_LEVELS: Array<[number, string]> = [
  [4, "excelente"],
  [3, "buena"],
  [2, "regular"],
  [1, "débil"],
  [0, "sin señal"],
];

export function signalWord(rssi?: number | null): string {
  const level = signalLevel(rssi);
  return SIGNAL_LEVELS.find(([l]) => l === level)?.[1] ?? "sin señal";
}

/** Colored bar of 4 cells, e.g. "●●●●" graded by strength. */
export function signalBars(rssi?: number | null): string {
  const level = signalLevel(rssi);
  return "●".repeat(level) + "○".repeat(4 - level);
}

/** "hace 5 min" style time ago. */
export function timeAgo(ts?: number | null): string {
  if (!ts) return "—";
  const ms = ts < 1e12 ? ts * 1000 : ts;
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 0) return "en unos segundos";
  if (diff < 60) return `hace ${Math.floor(diff)} seg`;
  if (diff < 3600) return `hace ${Math.floor(diff / 60)} min`;
  if (diff < 86400) return `hace ${Math.floor(diff / 3600)} h`;
  return `hace ${Math.floor(diff / 86400)} d`;
}

/** Readable date + time, e.g. "29 ago 2026, 09:15". */
export function formatDateTime(ts?: number | null): string {
  if (!ts) return "—";
  const ms = ts < 1e12 ? ts * 1000 : ts;
  const d = new Date(ms);
  const date = d.toLocaleDateString("es", { day: "2-digit", month: "short", year: "numeric" });
  const time = d.toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" });
  return `${date}, ${time}`;
}

/** Readable duration from seconds, e.g. "2 h 15 min" / "45 seg" / "3 d 4 h". */
export function formatDuration(seconds?: number | null): string {
  if (seconds == null) return "—";
  const s = Math.round(seconds);
  if (s < 60) return `${s} seg`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ${m % 60} min`;
  const d = Math.floor(h / 24);
  return `${d} d ${h % 24} h`;
}

/** Readable duration between two epochs (ms or seconds). */
export function durationBetween(start?: number | null, end?: number | null): string {
  if (!start || !end) return "—";
  const a = start < 1e12 ? start * 1000 : start;
  const b = end < 1e12 ? end * 1000 : end;
  return formatDuration((b - a) / 1000);
}

/**
 * Friendliest available name for a device. Prefers a hostname, then the
 * manufacturer/brand/model/type chain, then a generic fallback. Never raw MAC.
 */
export function deviceName(
  d: Device,
  detailed?: DetailedDevice | null
): string {
  const host = detailed?.hostname || d.hostname;
  if (host) return host;

  const parts = [
    detailed?.manufacturer,
    detailed?.brand,
    detailed?.model_guess,
    detailed?.device_class && detailed.device_class !== "Unknown"
      ? detailed.device_class
      : undefined,
  ].filter((x): x is string => Boolean(x));
  if (parts.length) return parts.join(" ");

  const band = bandLabel(d.band || detailed?.band);
  return band ? `Dispositivo sin identificar (${band})` : "Dispositivo sin identificar";
}

/** Short secondary line for a device (type + band). */
export function deviceSubtitle(d: Device, detailed?: DetailedDevice | null): string {
  const parts: string[] = [];
  const deviceClass =
    detailed?.device_class && detailed.device_class !== "Unknown"
      ? detailed.device_class
      : undefined;
  if (deviceClass) parts.push(deviceClass);
  const band = bandLabel(d.band || detailed?.band);
  if (band) parts.push(band);
  if (detailed?.operating_standard) parts.push(detailed.operating_standard);
  return parts.join(" · ");
}

/** Friendliest name for a network/AP: the SSID, or a generic label. */
export function networkName(n: Network): string {
  if (n.ssid) return n.ssid;
  const band = bandLabel(n.band);
  return band ? `Red Wi-Fi sin nombre (${band})` : "Red Wi-Fi sin nombre";
}

/** Short secondary line for a network (band + security + channel). */
export function networkSubtitle(n: Network): string {
  const parts: string[] = [];
  const band = bandLabel(n.band);
  if (band) parts.push(band);
  if (n.security) parts.push(n.security);
  if (n.channel != null) parts.push(`canal ${n.channel}`);
  return parts.join(" · ");
}

/** Weight used to rank proximity: lower = closer. */
export function proximityRank(proximity?: string | null): number {
  const zone = (proximity || "").toLowerCase();
  switch (zone) {
    case "immediate":
      return 0;
    case "near":
      return 1;
    case "medium":
      return 2;
    case "far":
    case "edge":
      return 3;
    default:
      return 4;
  }
}
