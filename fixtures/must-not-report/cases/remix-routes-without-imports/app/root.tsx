import { Links, Meta, Outlet, Scripts } from "@remix-run/react";

export default function App() {
  return (
    <html>
      <head>
        <Meta />
        <Links />
      </head>
      <body>
        <Outlet />
        <Scripts />
      </body>
    </html>
  );
}

export function CatchBoundary() {
  return <main>Caught</main>;
}

export const shouldReload = () => false;
export const unstable_shouldReload = () => false;
