import { useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, Smartphone, Save } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

import { DeviceSignalChart } from "@/components/device-signal-chart";
import { useRealtime } from "@/lib/realtime";
import {
  fetchAllDevices,
  fetchTimeline,
  fetchDeviceIdentity,
  updateDeviceIdentity,
  type DetailedDevice,
} from "@/lib/api";

function timeAgo(ms?: number | null) {
  if (ms == null) return "—";
  const diff = Math.floor(Date.now() - ms) / 1000;
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
  return `${Math.floor(diff / 86400)}d`;
}

function fmtDate(ms?: number | null) {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString();
}

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      <span className="text-sm font-medium text-foreground">{value}</span>
    </div>
  );
}

export function DeviceDetailView() {
  const { deviceId } = useParams({ from: "/devices/$deviceId" });
  const navigate = useNavigate();
  const live = useRealtime();
  const queryClient = useQueryClient();

  const all = useQuery<DetailedDevice[]>({
    queryKey: ["all-devices"],
    queryFn: fetchAllDevices,
  });
  const timeline = useQuery({
    queryKey: ["timeline"],
    queryFn: fetchTimeline,
  });
  const identity = useQuery({
    queryKey: ["device-identity", deviceId],
    queryFn: () => fetchDeviceIdentity(deviceId),
  });

  const [alias, setAlias] = useState("");
  const [owner, setOwner] = useState("");
  const [room, setRoom] = useState("");
  const [tags, setTags] = useState("");
  const [notes, setNotes] = useState("");

  useEffect(() => {
    if (identity.data) {
      setAlias(identity.data.alias || "");
      setOwner(identity.data.owner || "");
      setRoom(identity.data.room || "");
      setTags(identity.data.tags || "");
      setNotes(identity.data.notes || "");
    }
  }, [identity.data]);

  const save = useMutation({
    mutationFn: () =>
      updateDeviceIdentity(deviceId, {
        alias: alias.trim() || null,
        owner: owner.trim() || null,
        room: room.trim() || null,
        tags: tags.trim() ? tags.trim() : null,
        notes: notes.trim() || null,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["device-identity", deviceId] });
      queryClient.invalidateQueries({ queryKey: ["all-devices"] });
    },
  });

  const liveDev = useMemo(() => live.devices.get(deviceId), [live.devices, deviceId]);
  const detail = useMemo(
    () => all.data?.find((d) => d.pseudonym === deviceId),
    [all.data, deviceId]
  );

  const points = useMemo(
    () =>
      [
        ...(timeline.data?.points || []),
        ...live.points,
      ]
        .filter((p) => p.pseudonym === deviceId)
        .sort((a, b) => a.ts - b.ts),
    [timeline.data, live.points, deviceId]
  );

  if (all.isLoading || timeline.isLoading) {
    return <div className="py-12 text-center text-muted-foreground">Cargando…</div>;
  }

  if (!liveDev && !detail && !identity.data) {
    return (
      <div className="space-y-4 md:space-y-6">
        <Button
          variant="outline"
          size="sm"
          className="gap-2"
          onClick={() => navigate({ to: "/devices" })}
        >
          <ArrowLeft className="h-4 w-4" />
          Volver
        </Button>
        <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
          Dispositivo no encontrado
        </div>
      </div>
    );
  }

  const name = detail?.alias || identity.data?.alias || detail?.hostname || liveDev?.hostname || deviceId;
  const deviceState = liveDev?.state ?? (detail?.status === "connected" ? "CONNECTED" : "ABSENT");
  const isConnected = deviceState !== "ABSENT" && deviceState !== "DISCONNECTED";

  return (
    <div className="space-y-4 md:space-y-6">
      <Button
        variant="outline"
        size="sm"
        className="gap-2"
        onClick={() => navigate({ to: "/devices" })}
      >
        <ArrowLeft className="h-4 w-4" />
        Volver
      </Button>

      <div className="flex items-start gap-4">
        <div className="rounded-lg border border-border bg-card p-3">
          <Smartphone className="h-6 w-6 text-primary" />
        </div>
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">{name}</h2>
          <p className="font-mono text-sm text-muted-foreground">
            {deviceId}
          </p>
        </div>
        <Badge
          variant={isConnected ? "default" : "secondary"}
          className={
            isConnected
              ? "ml-auto bg-[var(--color-online)]/10 text-[var(--color-online)]"
              : "ml-auto bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
          }
        >
          {deviceState === "RF_PRESENT" ? "RF presente" : isConnected ? "conectado" : deviceState ? deviceState.toLowerCase() : "desconectado"}
        </Badge>
      </div>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Estado actual
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
            <Field
              label="Estado"
              value={
                <Badge
                  variant="outline"
                  className={
                    isConnected
                      ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                      : "bg-[var(--color-offline)]/10 text-[var(--color-offline)]"
                  }
                >
                  {deviceState === "RF_PRESENT" ? "RF presente" : deviceState ? deviceState.charAt(0).toUpperCase() + deviceState.slice(1).toLowerCase() : "Desconectado"}
                </Badge>
              }
            />
            <Field
              label="Señal"
              value={
                liveDev?.last_signal != null
                  ? `${liveDev.last_signal} dBm`
                  : detail?.signal_strength != null
                  ? `${detail.signal_strength} dBm`
                  : "—"
              }
            />
            <Field
              label="Banda"
              value={liveDev?.band || detail?.band || "—"}
            />
            <Field
              label="AP"
              value={detail?.bssid_pseudonym || detail?.bssid_manufacturer || "—"}
            />
            <Field label="Primera vez" value={fmtDate(detail?.first_seen)} />
            <Field label="Última vez" value={timeAgo(liveDev?.last_seen || detail?.last_seen)} />
            <Field
              label="Observaciones"
              value={liveDev?.event_count ?? detail?.observations ?? "—"}
            />
            <Field
              label="Fabricante"
              value={detail?.manufacturer || detail?.brand || "Desconocido"}
            />
            <Field label="Clase" value={detail?.device_class || "Desconocido"} />
            <Field label="Tipo de MAC" value={detail?.mac_type || "—"} />
            <Field
              label="Huella"
              value={detail?.fingerprint_model || "—"}
            />
            <Field
              label="Confianza"
              value={detail?.confidence_label || "—"}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Etiquetas
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Alias
              </span>
              <input
                type="text"
                value={alias}
                onChange={(e) => setAlias(e.target.value)}
                placeholder="Ej: Celular de Juan"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
              />
            </div>
            <div className="space-y-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Responsable / dueño
              </span>
              <input
                type="text"
                value={owner}
                onChange={(e) => setOwner(e.target.value)}
                placeholder="Ej: Juan"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
              />
            </div>
            <div className="space-y-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Ubicación
              </span>
              <input
                type="text"
                value={room}
                onChange={(e) => setRoom(e.target.value)}
                placeholder="Ej: Sala"
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
              />
            </div>
            <div className="space-y-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Tags (JSON)
              </span>
              <input
                type="text"
                value={tags}
                onChange={(e) => setTags(e.target.value)}
                placeholder='Ej: ["iot", "camara"]'
                className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
              />
            </div>
            <div className="space-y-2 sm:col-span-2">
              <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Notas
              </span>
              <textarea
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder="Notas adicionales..."
                rows={3}
                className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground outline-none ring-ring placeholder:text-muted-foreground focus:ring-1"
              />
            </div>
          </div>
          <div className="mt-4 flex items-center gap-3">
            <Button
              size="sm"
              className="gap-2"
              onClick={() => save.mutate()}
              disabled={save.isPending}
            >
              <Save className="h-4 w-4" />
              {save.isPending ? "Guardando..." : "Guardar"}
            </Button>
            {save.isError && (
              <span className="text-xs text-destructive">
                Error al guardar
              </span>
            )}
            {save.isSuccess && (
              <span className="text-xs text-green-600">Guardado</span>
            )}
          </div>
        </CardContent>
      </Card>

      <DeviceSignalChart pseudonym={deviceId} points={points} />

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Historial de sesiones
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            El historial de sesiones requiere soporte del backend. Se muestra el
            historial de señal como referencia de la actividad observada.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
