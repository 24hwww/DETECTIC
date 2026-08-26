import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { PageHeader } from "@/components/page-header";

export function SessionsView() {
  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Sessions"
        description="Sesiones de dispositivos detectadas"
      />
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Sesiones por dispositivo
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="py-8 text-center text-sm text-muted-foreground">
            El backend no expone aún datos de sesiones. Esta sección mostrará
            duración, primer/último seen y recurrencia por pseudónimo.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
