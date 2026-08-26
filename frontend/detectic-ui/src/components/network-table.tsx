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
import type { Network } from "@/lib/api";

const columnHelper = createColumnHelper<Network>();

const columns = [
  columnHelper.accessor("ap_id", {
    header: "AP",
    cell: (info) => (
      <code className="text-xs font-mono">{info.getValue().slice(0, 16)}</code>
    ),
  }),
  columnHelper.accessor("ssid", {
    header: "SSID",
    cell: (info) => info.getValue() || "—",
  }),
  columnHelper.accessor("status", {
    header: "Estado",
    cell: (info) =>
      info.getValue() === "ONLINE" ? (
        <Badge
          variant="default"
          className="bg-green-500/10 text-green-500 hover:bg-green-500/10"
        >
          online
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
  columnHelper.accessor("band", {
    header: "Banda",
    cell: (info) => info.getValue() || "—",
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

export function NetworkTable({ networks }: { networks: Network[] }) {
  const [sorting, setSorting] = useState<SortingState>([]);

  const table = useReactTable({
    data: networks,
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
          Redes Wi-Fi observadas (APs)
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
