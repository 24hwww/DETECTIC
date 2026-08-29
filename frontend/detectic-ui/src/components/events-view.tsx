import { LiveFeed } from "@/components/live-feed";
import { PageHeader } from "@/components/page-header";

export function EventsView() {
  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Eventos"
        description="Eventos del sensor en tiempo real"
      />
      <LiveFeed />
    </div>
  );
}
