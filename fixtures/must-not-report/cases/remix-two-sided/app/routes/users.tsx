import { useLoaderData } from "@remix-run/react";

export async function loader() {
  return { users: [] };
}

export default function Users() {
  const data = useLoaderData();
  return <div>{String(data)}</div>;
}

export function unusedRouteHelper() {
  return 1;
}
