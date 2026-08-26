export function mergeLive<T extends { device_id?: string; ap_id?: string }>(
  fetched: T[],
  live: Map<string, T>
): T[] {
  const map = new Map<string, T>();
  for (const d of fetched) {
    const key = d.device_id || d.ap_id || "";
    if (key) map.set(key, d);
  }
  for (const [key, d] of live) {
    map.set(key, { ...map.get(key), ...d } as T);
  }
  return Array.from(map.values()).sort((a, b) => {
    const ta = (a as any).last_seen ?? 0;
    const tb = (b as any).last_seen ?? 0;
    return tb - ta;
  });
}
