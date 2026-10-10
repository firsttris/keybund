import { DoctorPanel } from "@/components/doctor-panel";

export function DoctorPage() {
  return (
    <>
      <div className="flex flex-col gap-1.5">
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">Doctor</h1>
        <p className="m-0 text-muted-foreground">Prüft Rechte, Include-Zeilen, Agent und ob alle Configs aktuell sind.</p>
      </div>
      <DoctorPanel className="max-w-[720px]" />
    </>
  );
}
