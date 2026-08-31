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
  deviceName,
  deviceSubtitle,
  durationBetween,
  proximityText,
  signalWord,
  timeAgo,
} from "@/lib/labels";
import type { Device, DetailedDevice } from "@/lib/api";

const columnHelper = createColumnHelper<Device>();

function makeColumns(identity: Map<string, DetailedDevice>) {
  const find = (d: Device) => identity.get(d.device_id);

  return [
    columnHelper.accessor("device_id", {
      header: "Dispositivo",
      cell: (info) => {
        const d = info.row.original;
        const id = find(d);
        return (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-foreground">
              {deviceName(d, id)}
            </div>
            <div className="truncate text-[11px] text-muted-foreground">
              {deviceSubtitle(d, id) || "Dispositivo"}
            </div>
          </div>
        );
      },
    }),
    columnHelper.accessor("state", {
      id: "state",
      header: "Estado",
      cell: (info) => {
        const state = info.getValue() as string | undefined;
        const online =
          info.row.original.connected ||
          state === "CONNECTED" ||
          state === "RF_PRESENT";
        return (
          <Badge
            variant={online ? "default" : "secondary"}
            className={
              online
                ? "bg-[var(--color-online)]/10 text-[var(--color-online)] hover:bg-[var(--color-online)]/10"
                : "bg-[var(--color-offline)]/10 text-[var(--color-offline)] hover:bg-[var(--color-offline)]/10"
            }
          >
            {online ? "conectado" : "no está"}
          </Badge>
        );
      },
    }),
    columnHelper.accessor("proximity", {
      header: "Distancia al router",
      cell: (info) => {
        const v = info.getValue();
        const text = proximityText(v);
        return <span>{text === "desconocido" ? "—" : text}</span>;
      },
    }),
    columnHelper.accessor("last_signal", {
      header: "Señal",
      cell: (info) => {
        const v = info.getValue();
        return (
          <div>
            <div className="text-foreground">{signalWord(v)}</div>
            {v != null && (
              <div className="text-[11px] text-muted-foreground">{v} dBm</div>
            )}
          </div>
        );
      },
    }),
    columnHelper.accessor("band", {
      header: "Banda",
      cell: (info) => {
        const b = bandLabel(info.getValue());
        return b || "—";
      },
    }),
    columnHelper.accessor("first_seen", {
      header: "Cuánto tiempo",
      cell: (info) => {
        const d = info.row.original;
        const dur = durationBetween(d.first_seen, d.last_seen);
        return <span>{dur === "—" ? "—" : dur}</span>;
      },
    }),
    columnHelper.accessor("last_seen", {
      header: "Última vez",
      cell: (info) => timeAgo(info.getValue()),
    }),
  ];
}

export function DeviceTable({
  devices,
  identity,
  onRowClick,
}: {
  devices: Device[];
  identity?: Map<string, DetailedDevice>;
  onRowClick?: (d: Device) => void;
}) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");
  const cols = makeColumns(identity ?? new Map());

  const table = useReactTable({
    data: devices,
    columns: cols,
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
          Dispositivos detectados
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
              placeholder="Buscar dispositivo, sensor..."
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
