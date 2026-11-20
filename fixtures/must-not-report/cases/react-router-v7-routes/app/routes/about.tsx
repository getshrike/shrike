import type { Route } from "./+types/about";

export async function action({ request }: Route.ActionArgs) {
  return request.formData();
}

export const clientMiddleware = [];

export default function About() {
  return <main>About</main>;
}
