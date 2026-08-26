import { useState } from "react";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type SortingState,
} from "@tanstack/react-table";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { Device } from "@/lib/api";

const columnHelper = createColumnHelper<Device>();

const columns = [
  columnHelper.accessor("device_id", {
    header: "Dispositivo",
    cell: (info) => (
      <code className="text-xs font-mono">{info.getValue()}</code>
    ),
  }),
  columnHelper.accessor("connected", {
    header: "Estado",
    cell: (info) =>
      info.getValue() ? (
        <Badge variant="default">online</Badge>
      ) : (
        <Badge variant="secondary">offline</Badge>
      ),
  }),
  columnHelper.accessor("last_signal", {
    header: "RSSI",
    cell: (info) => {
      const v = info.getValue();
      return v != null ? `${v} dBm` : "—";
    },
  }),
  columnHelper.accessor("sensor_id", {
    header: "Sensor",
    cell: (info) => info.getValue() || "—",
  }),
];

export function DeviceTable({ devices }: { devices: Device[] }) {
  const [sorting, setSorting] = useState<SortingState>([]);

  const table = useReactTable({
    data: devices,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Dispositivos
        </CardTitle>
      </CardHeader>
      <CardContent className="p-0">
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
