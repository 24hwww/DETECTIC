import { useState } from "react";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  useReactTable,
  type SortingState,
} from "@tanstack/react-table";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Search } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  bandLabel,
  durationBetween,
  formatDateTime,
  networkName,
  networkSubtitle,
  proximityText,
  signalWord,
} from "@/lib/labels";
import type { Network } from "@/lib/api";

const columnHelper = createColumnHelper<Network>();

const columns = [
  columnHelper.accessor("ssid", {
    header: "Red (AP)",
    cell: (info) => {
      const n = info.row.original;
      return (
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">
            {networkName(n)}
          </div>
          <div className="truncate text-[11px] text-muted-foreground">
            {networkSubtitle(n)}
          </div>
        </div>
      );
    },
  }),
  columnHelper.accessor("status", {
    header: "Estado",
    cell: (info) =>
      info.getValue() === "ONLINE" ? (
        <Badge
          variant="default"
          className="bg-[var(--color-online)]/10 text-[var(--color-online)] hover:bg-[var(--color-online)]/10"
        >
          online
        </Badge>
      ) : (
        <Badge
          variant="secondary"
          className="bg-[var(--color-offline)]/10 text-[var(--color-offline)] hover:bg-[var(--color-offline)]/10"
        >
          sin señal
        </Badge>
      ),
  }),
  columnHelper.accessor("band", {
    header: "Banda",
    cell: (info) => bandLabel(info.getValue()) || "—",
  }),
  columnHelper.accessor("first_seen", {
    header: "Cuándo se detectó",
    cell: (info) => formatDateTime(info.getValue()),
  }),
  columnHelper.accessor("last_seen", {
    header: "Cuánto tiempo",
    cell: (info) => {
      const n = info.row.original;
      const dur = durationBetween(n.first_seen, n.last_seen);
      return dur === "—" ? "—" : dur;
    },
  }),
  columnHelper.accessor("proximity", {
    header: "Distancia aprox.",
    cell: (info) =>
      proximityText(
        info.getValue(),
        info.row.original.proximity_detail
      ),
  }),
  columnHelper.accessor("current_signal", {
    header: "Señal",
    cell: (info) => {
      const v = info.getValue() ?? info.row.original.last_signal;
      return signalWord(v);
    },
  }),
];

export function NetworkTable({
  networks,
  onRowClick,
}: {
  networks: Network[];
  onRowClick?: (n: Network) => void;
}) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");

  const table = useReactTable({
    data: networks,
    columns,
    state: { sorting, globalFilter },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
  });

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Redes Wi-Fi detectadas
        </CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        <div className="border-b border-border p-3">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <input
              type="search"
              value={globalFilter}
              onChange={(e) => setGlobalFilter(e.target.value)}
              placeholder="Buscar red, banda, sensor..."
              className="h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-xs text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
            />
          </div>
        </div>
        <div className="max-h-[420px] overflow-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted text-muted-foreground">
              {table.getHeaderGroups().map((hg) => (
                <tr key={hg.id}>
                  {hg.headers.map((h) => (
                    <th
                      key={h.id}
                      className="cursor-pointer p-3 text-left font-medium"
                      onClick={h.column.getToggleSortingHandler()}
                    >
                      {flexRender(h.column.columnDef.header, h.getContext())}
                      {{
                        asc: " ↑",
                        desc: " ↓",
                      }[h.column.getIsSorted() as string] ?? null}
                    </th>
                  ))}
                </tr>
              ))}
            </thead>
            <tbody>
              {table.getRowModel().rows.map((row) => (
                <tr
                  key={row.id}
                  className={cn(
                    "border-b border-border last:border-0",
                    onRowClick && "cursor-pointer hover:bg-muted/50"
                  )}
                  onClick={() => onRowClick?.(row.original)}
                >
                  {row.getVisibleCells().map((cell) => (
                    <td key={cell.id} className="p-3">
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </CardContent>
    </Card>
  );
}
