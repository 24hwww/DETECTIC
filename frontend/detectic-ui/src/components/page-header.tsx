import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

export function PageHeader({
  title,
  description,
  onRefresh,
}: {
  title: string;
  description: string;
  onRefresh?: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h2 className="text-2xl font-semibold tracking-tight">{title}</h2>
        <p className="text-sm text-muted-foreground">{description}</p>
      </div>
      <div className="flex items-center gap-2">
        <div className="rounded-md border border-border bg-card px-3 py-1.5 text-xs text-muted-foreground">
          Últimas 24h
        </div>
        <Button
          variant="outline"
          size="sm"
          className="h-8 gap-2 text-xs"
          onClick={onRefresh}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refrescar
        </Button>
      </div>
    </div>
  );
}
