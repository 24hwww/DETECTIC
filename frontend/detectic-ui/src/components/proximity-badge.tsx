import { cn, parseProximityDetail, proximityZoneClass } from "@/lib/utils";

export function ProximityBadge({
  proximity,
  detail,
}: {
  proximity?: string | null;
  detail?: string | Record<string, unknown> | null;
}) {
  if (!proximity) return <span className="text-muted-foreground">—</span>;
  const { zone, distance } = parseProximityDetail(detail);
  const cls = proximityZoneClass(zone);
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded border px-2 py-0.5 text-xs font-medium",
        cls
      )}
      title={detail && typeof detail === "string" ? detail : undefined}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {proximity}
      {distance != null ? (
        <span className="opacity-80">· {distance.toFixed(1)} m</span>
      ) : null}
    </span>
  );
}
