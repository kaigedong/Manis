import { host, mount, Text, View } from "nsh";
import { NodesPage } from "./nodes";
import { SubscriptionsPage } from "./subscriptions";
import { RulesPage } from "./rules";
import { SettingsPage } from "./settings";

const page = ((host.params ?? {}) as { page?: string }).page ?? "nodes";

function App() {
  return (
    <View class="flex-1 bg-background">
      {page === "subscriptions" ? (
        <SubscriptionsPage />
      ) : page === "rules" ? (
        <RulesPage />
      ) : page === "settings" ? (
        <SettingsPage />
      ) : (
        <NodesPage />
      )}
    </View>
  );
}

mount(App);
