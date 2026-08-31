import { useEffect, useRef } from "react";
import { useRealtime, extractDevice, parseOuterPayload } from "@/lib/realtime";
import { useToast } from "@/components/ui/toast";
import { bandLabel, deviceName, networkName, proximityText } from "@/lib/labels";
import type { Device, DetailedDevice, Network } from "@/lib/api";

function isConnected(d: Device): boolean {
  return (
    d.connected ||
    (d.state != null &&
      d.state !== "ABSENT" &&
      d.state !== "DISCONNECTED")
  );
}

export function LiveToasts({
  devices,
  networks,
  identity,
}: {
  devices?: Device[];
  networks?: Network[];
  identity?: Map<string, DetailedDevice>;
}) {
  const { events } = useRealtime();
  const addToast = useToast().add;

  const knownDevices = useRef<Set<string>>(new Set());
  const knownNetworks = useRef<Set<string>>(new Set());
  const connectedDevices = useRef<Set<string>>(new Set());
  const processed = useRef<Set<string>>(new Set());
  const seeded = useRef(false);

  useEffect(() => {
    if (seeded.current) return;
    (devices || []).forEach((d) => {
      knownDevices.current.add(d.device_id);
      if (isConnected(d)) connectedDevices.current.add(d.device_id);
    });
    (networks || []).forEach((n) => knownNetworks.current.add(n.ap_id));
    seeded.current = true;
  }, [devices, networks]);

  useEffect(() => {
    for (const ev of events) {
      const { outer, inner, type } = parseOuterPayload(ev);
      const id = String(
        outer.device_id ||
          outer.ap_id ||
          inner.device_id ||
          inner.bssid_pseudonym ||
          inner.ap_id ||
          ""
      );
      const ts = ev.server_time || ev.observed_at || "";
      const key = `${type}::${id}::${ts}`;
      if (!id || processed.current.has(key)) continue;
      processed.current.add(key);

      if (type === "device.connected") {
        const name = deviceName(
          extractDevice(ev) || { device_id: id, connected: true },
          identity?.get(id)
        );
        if (!knownDevices.current.has(id)) {
          knownDevices.current.add(id);
          connectedDevices.current.add(id);
          addToast({
            title: "Dispositivo conectado",
            description: `${name} se conectó a la red`,
            type: "device",
          });
        } else if (!connectedDevices.current.has(id)) {
          connectedDevices.current.add(id);
          addToast({
            title: "Dispositivo de nuevo en línea",
            description: `${name} volvió a conectarse`,
            type: "device",
          });
        }
      } else if (type === "device.disconnected") {
        connectedDevices.current.delete(id);
      } else if (type === "network.detected") {
        if (!knownNetworks.current.has(id)) {
          knownNetworks.current.add(id);
          const net: Network = {
            ap_id: id,
            ssid: inner.ssid,
            band: inner.band,
            status: inner.status,
            proximity: inner.proximity,
          };
          const name = networkName(net);
          const band = bandLabel(net.band);
          addToast({
            title: "Nueva red detectada",
            description: `${name}${band ? ` · banda ${band}` : ""} · ${proximityText(
              net.proximity
            )}`,
            type: "network",
          });
        }
      }
    }
  }, [events, identity, addToast]);

  return null;
}
