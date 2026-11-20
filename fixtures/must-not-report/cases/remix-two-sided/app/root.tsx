import { Outlet } from "@remix-run/react";

export default function Root() {
  return <Outlet />;
}

export function loader() {
  return null;
}
