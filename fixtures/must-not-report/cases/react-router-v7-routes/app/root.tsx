import { Outlet } from "react-router";

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html>
      <body>{children}</body>
    </html>
  );
}

export default function Root() {
  return <Outlet />;
}
