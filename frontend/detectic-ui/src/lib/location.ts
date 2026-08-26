export function sourceColor(source?: string | null) {
  switch (source) {
    case "gps":
      return "bg-emerald-500";
    case "browser":
      return "bg-sky-500";
    case "manual":
      return "bg-amber-500";
    default:
      return "bg-muted";
  }
}
