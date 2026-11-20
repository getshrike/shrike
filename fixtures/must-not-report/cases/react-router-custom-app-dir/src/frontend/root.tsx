import { Outlet } from "react-router";

export function Layout({ children }: { children: React.ReactNode }) {
  return <html>{children}</html>;
}

export default function Root() {
  return <Outlet />;
}

export function unusedRootHelper() {
  return 1;
}
