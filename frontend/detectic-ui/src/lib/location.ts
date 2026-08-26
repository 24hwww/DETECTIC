export function sourceColor(source?: string | null) {
  switch (source) {
    case "gps":
      return "bg-emerald-600";
    case "browser":
      return "bg-sky-600";
    case "manual":
      return "bg-amber-600";
    default:
      return "bg-muted";
  }
}
