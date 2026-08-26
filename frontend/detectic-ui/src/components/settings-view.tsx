import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { PageHeader } from "@/components/page-header";

export function SettingsView() {
  return (
    <div className="space-y-4 md:space-y-6">
      <PageHeader
        title="Settings"
        description="Configuración del dashboard y sensores"
      />
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Preferencias
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            La configuración de sensores, alertas y exportación estará disponible
            aquí cuando el backend exponga los endpoints correspondientes.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
