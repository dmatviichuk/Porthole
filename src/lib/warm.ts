import { holdDiscovery } from "./discovery";
import {
  CRONJOB,
  DAEMONSET,
  DEPLOYMENT,
  INGRESS,
  JOB,
  NAMESPACE,
  NODE,
  POD,
  PVC,
  SERVICE,
  STATEFULSET,
} from "./kinds";
import { holdMetrics } from "./metricsStore";
import { holdResources } from "./watchStore";

/** What the sidebar, Applications, Overview and resource pages read. */
const CORE = [NAMESPACE, POD, DEPLOYMENT, STATEFULSET, DAEMONSET, JOB, CRONJOB, NODE, SERVICE, INGRESS, PVC];

/**
 * Starts everything the main views need as soon as a context is selected, and keeps it live
 * while the context stays selected, so no view waits for a first list.
 */
export function warmContext(context: string): () => void {
  const releases = [...CORE.map((type) => holdResources(context, type)), holdDiscovery(context), holdMetrics(context)];
  return () => {
    for (const release of releases) release();
  };
}
