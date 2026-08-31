import { useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, Smartphone, Save, Activity } from "lucide-react";
import { proximityText, signalWord, bandLabel } from "@/lib/labels";
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
  fetchDevicePatterns,
  updateDeviceTrust,
  type DetailedDevice,
  type DevicePattern,
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
  const pattern = useQuery<DevicePattern | null>({
    queryKey: ["device-patterns", deviceId],
    queryFn: () => fetchDevicePatterns(deviceId, 168),
  });

  const trust = useMutation({
    mutationFn: (status: 'known' | 'ignored' | 'unknown') => updateDeviceTrust(deviceId, status),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["all-devices"] });
      queryClient.invalidateQueries({ queryKey: ["unknown-devices"] });
    },
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
              label="AP"
              value={detail?.bssid_pseudonym || detail?.bssid_manufacturer || "—"}
            />
            <Field
              label="Confianza"
              value={
                <div className="flex items-center gap-2">
                  <Badge
                    variant="outline"
                    className={
                      detail?.trust_status === 'known'
                        ? "bg-[var(--color-online)]/10 text-[var(--color-online)]"
                        : detail?.trust_status === 'ignored'
                        ? "bg-muted text-muted-foreground"
                        : "bg-[var(--color-destructive)]/10 text-[var(--color-destructive)]"
                    }
                  >
                    {detail?.trust_status === 'known' ? 'Conocido' : detail?.trust_status === 'ignored' ? 'Ignorado' : 'Desconocido'}
                  </Badge>
                  {detail?.trust_status !== 'known' && (
                    <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={() => trust.mutate('known')} disabled={trust.isPending}>
                      Marcar conocido
                    </Button>
                  )}
                  {detail?.trust_status !== 'ignored' && (
                    <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={() => trust.mutate('ignored')} disabled={trust.isPending}>
                      Ignorar
                    </Button>
                  )}
                </div>
              }
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
              label="Confianza ID"
              value={detail?.confidence_label || "—"}
            />
            <Field
              label="Proximidad"
              value={proximityText(liveDev?.proximity, liveDev?.proximity_detail) || "—"}
            />
            <Field
              label="Tendencia"
              value={
                liveDev?.proximity_detail?.trend_arrow
                  ? `${liveDev.proximity_detail.trend_arrow} ${liveDev.proximity_detail.trend_label || liveDev.trend || "—"}`
                  : liveDev?.trend || "—"
              }
            />
            <Field
              label="Intensidad"
              value={liveDev?.heat != null ? `${liveDev.heat}/100` : "—"}
            />
            <Field
              label="Distancia"
              value={
                liveDev?.distance_m != null
                  ? `~${Math.round(liveDev.distance_m)} m`
                  : liveDev?.proximity_detail?.distance_m != null
                  ? `~${Math.round(liveDev.proximity_detail.distance_m)} m`
                  : "—"
              }
            />
            <Field
              label="RSSI"
              value={
                liveDev?.rssi_dbm != null
                  ? `${liveDev.rssi_dbm} dBm · ${signalWord(liveDev.rssi_dbm)}`
                  : liveDev?.last_signal != null
                  ? `${liveDev.last_signal} dBm · ${signalWord(liveDev.last_signal)}`
                  : "—"
              }
            />
            <Field
              label="Banda"
              value={bandLabel(liveDev?.band) || bandLabel(detail?.band) || "—"}
            />
            <Field
              label="Confianza proximidad"
              value={
                liveDev?.proximity_detail?.confidence != null
                  ? `${Math.round(liveDev.proximity_detail.confidence * 100)}%${
                      liveDev.proximity_detail.samples ? ` · ${liveDev.proximity_detail.samples} muestras` : ""
                    }`
                  : "—"
              }
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
          <CardTitle className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            <Activity className="h-4 w-4" />
            Patrones de actividad (últimos 7 días)
          </CardTitle>
        </CardHeader>
        <CardContent>
          {pattern.isLoading ? (
            <p className="text-sm text-muted-foreground">Cargando…</p>
          ) : pattern.data ? (
            <div className="space-y-4">
              <div>
                <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">Horas pico</p>
                <p className="text-sm">
                  {pattern.data.top_hours.map((h) => `${String(h.hour).padStart(2, "0")}h (${Math.round(h.ratio * 100)}%)`).join(", ") || "Sin datos"}
                </p>
              </div>
              <div>
                <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">Actividad por hora</p>
                <div className="mt-2 flex h-16 items-end gap-1">
                  {pattern.data.hour_counts?.map((c, i) => {
                    const max = Math.max(1, ...(pattern.data?.hour_counts || []));
                    return (
                      <div key={i} className="flex flex-1 flex-col items-center gap-1">
                        <div
                          className="w-full rounded-sm bg-primary/60"
                          style={{ height: `${Math.round((c / max) * 100)}%`, minHeight: c > 0 ? 2 : 0 }}
                          title={`${String(i).padStart(2, "0")}h: ${c}`}
                        />
                        {i % 4 === 0 && <span className="text-[8px] text-muted-foreground">{i}h</span>}
                      </div>
                    );
                  })}
                </div>
              </div>
              <div>
                <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">Días de la semana</p>
                <div className="mt-2 flex gap-1">
                  {pattern.data.weekday_counts.map((c, i) => {
                    const max = Math.max(1, ...pattern.data.weekday_counts);
                    return (
                      <div key={i} className="flex flex-1 flex-col items-center gap-1">
                        <div
                          className="w-6 rounded-sm bg-primary/60"
                          style={{ height: `${Math.round((c / max) * 60)}px`, minHeight: c > 0 ? 2 : 0 }}
                          title={`${["L", "M", "X", "J", "V", "S", "D"][i]}: ${c}`}
                        />
                        <span className="text-[9px] text-muted-foreground">{["L", "M", "X", "J", "V", "S", "D"][i]}</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">Sin datos de patrones.</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
