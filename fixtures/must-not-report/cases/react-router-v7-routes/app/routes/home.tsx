import type { Route } from "./+types/home";

export function loader({}: Route.LoaderArgs) {
  return { title: "Home" };
}

export const middleware = [];
export const unstable_middleware = [];

export default function Home({ loaderData }: Route.ComponentProps) {
  return <main>{loaderData.title}</main>;
}
