import { lazy, Suspense } from "react";
import { useUsageEvents } from "./hooks/useUsage";

const Dashboard = lazy(() =>
  import("./dashboard/Dashboard").then((module) => ({
    default: module.Dashboard,
  })),
);
const FloatingPanel = lazy(() =>
  import("./floating/FloatingPanel").then((module) => ({
    default: module.FloatingPanel,
  })),
);

export function App() {
  useUsageEvents();
  const surface = new URLSearchParams(window.location.search).get("surface");
  return (
    <Suspense
      fallback={<div className="surface-boot">LOCAL INSTRUMENT BOOT</div>}
    >
      {surface === "floating" ? <FloatingPanel /> : <Dashboard />}
    </Suspense>
  );
}
