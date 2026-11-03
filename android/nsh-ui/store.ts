import { host, signal } from "nsh";

export type Tab = "connection" | "nodes" | "subscriptions" | "rules" | "settings";

export type RootData = {
  pending: boolean;
  nodes: any;
  subscriptions: any;
  rules: any;
  settings: any;
  connection: any;
};

const params = (host.params ?? {}) as {
  data?: RootData;
  embedded?: boolean;
  tab?: Tab;
  topInset?: number;
  bottomInset?: number;
  width?: number;
  height?: number;
};

export const embedded = params.embedded === true;
export const topInset = params.topInset ?? 0;
export const bottomInset = params.bottomInset ?? 0;
/** The host's window width in points (0 when it does not report one). */
export const windowWidth = params.width ?? 0;
/** The width the desktop shell last laid itself out at (0 before the first layout). */
export const viewportWidth = signal(0);

/** The width to pick the layout class from: the live layout, else the host's window. */
export function layoutWidth(): number {
  return viewportWidth() || windowWidth;
}
export const root = signal<RootData>(
  params.data ?? {
    pending: false,
    nodes: { global: { label: "全局", current: null, total: 0 }, sources: [], policyGroups: [] },
    subscriptions: { subscriptions: [], nodes: [], groups: [] },
    rules: { rules: [], kinds: [], policies: [], warnings: [], summary: "" },
    settings: {},
    connection: {},
  },
);
export const tab = signal<Tab>(params.tab ?? "connection");
export const toast = signal("");

host.on("app.updated", (payload) => {
  const next = (payload as { data?: RootData; message?: string } | undefined);
  if (next?.data) root.set(next.data);
  if (next?.message) toast.set(next.message);
});
