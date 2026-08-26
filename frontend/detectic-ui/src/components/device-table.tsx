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
import type { Device } from "@/lib/api";

function timeAgo(ms?: number) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

const columnHelper = createColumnHelper<Device>();

const columns = [
  columnHelper.accessor("device_id", {
    header: "Dispositivo",
    cell: (info) => (
      <code className="text-xs font-mono">{info.getValue().slice(0, 20)}</code>
    ),
  }),
  columnHelper.accessor("hostname", {
    header: "Hostname",
    cell: (info) => info.getValue() || "—",
  }),
  columnHelper.accessor("connected", {
    header: "Estado",
    cell: (info) =>
      info.getValue() ? (
        <Badge
          variant="default"
          className="bg-green-500/10 text-green-500 hover:bg-green-500/10"
        >
          connected
        </Badge>
      ) : (
        <Badge
          variant="secondary"
          className="bg-red-500/10 text-red-500 hover:bg-red-500/10"
        >
          offline
        </Badge>
      ),
  }),
  columnHelper.accessor("last_signal", {
    header: "RSSI",
    cell: (info) => {
      const v = info.getValue();
      return v != null ? `${v} dBm` : "—";
    },
  }),
  columnHelper.accessor("band", {
    header: "Banda",
    cell: (info) => info.getValue() || "—",
  }),
  columnHelper.accessor("sensor_id", {
    header: "Sensor",
    cell: (info) => info.getValue() || "—",
  }),
  columnHelper.accessor("last_seen", {
    header: "Última vez",
    cell: (info) => timeAgo(info.getValue()),
  }),
  columnHelper.accessor("event_count", {
    header: "Eventos",
    cell: (info) => info.getValue() ?? "—",
  }),
];

export function DeviceTable({ devices }: { devices: Device[] }) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");

  const table = useReactTable({
    data: devices,
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
          Dispositivos
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
              placeholder="Buscar dispositivo, hostname, sensor..."
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
                <tr key={row.id} className="border-b border-border last:border-0">
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
