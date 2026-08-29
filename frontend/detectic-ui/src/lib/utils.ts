import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function parseProximityDetail(detail: unknown): {
  distance?: number;
  zone?: string;
  trend?: string;
} {
  if (!detail) return {};
  let obj: any = detail;
  if (typeof detail === "string") {
    try { obj = JSON.parse(detail); } catch { return {}; }
  }
  if (typeof obj !== "object" || obj === null) return {};
  return {
    distance: typeof obj.distance_m === "number" ? obj.distance_m : undefined,
    zone: typeof obj.zone === "string" ? obj.zone : undefined,
    trend: typeof obj.trend === "string" ? obj.trend : undefined,
  };
}

export function proximityZoneClass(zone?: string | null): string {
  switch (zone) {
    case "immediate":
    case "near":
      return "prox-near";
    case "medium":
      return "prox-medium";
    case "far":
    case "edge":
      return "prox-far";
    default:
      return "prox-unknown";
  }
}
