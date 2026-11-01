import { host, mount, Text, View } from "nsh";
import { NodesPage } from "./nodes";
import { SubscriptionsPage } from "./subscriptions";

const page = ((host.params ?? {}) as { page?: string }).page ?? "nodes";

function App() {
  return (
    <View class="flex-1 bg-background">
      {page === "subscriptions" ? <SubscriptionsPage /> : <NodesPage />}
    </View>
  );
}

mount(App);
