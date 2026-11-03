import { t } from "./copy";

/** A routing mode's label: the host sends the wire value. */
export function modeLabel(mode: string): string {
  switch (mode) {
    case "rule":
      return t("conn.mode.rule");
    case "global":
      return t("conn.mode.global");
    case "direct":
      return t("conn.mode.direct");
    default:
      return mode;
  }
}

/** A policy group's label: the controller sends the Mihomo type. */
export function strategyLabel(kind: string): string {
  switch (kind.toLowerCase()) {
    case "selector":
      return t("strategy.selector");
    case "urltest":
      return t("strategy.urltest");
    case "fallback":
      return t("strategy.fallback");
    case "loadbalance":
      return t("strategy.loadBalance");
    case "relay":
      return t("strategy.relay");
    default:
      return kind;
  }
}

/** A source kind's label. */
export function sourceKindLabel(kind: string): string {
  switch (kind) {
    case "subscription":
      return t("nodes.subscription");
    case "manual":
      return t("nodes.manual");
    default:
      return kind;
  }
}

/** The node DNS mode's label. */
export function dnsLabel(mode: string): string {
  switch (mode) {
    case "doh":
      return t("settings.dns.doh");
    case "system":
      return t("settings.dns.system");
    default:
      return mode;
  }
}
