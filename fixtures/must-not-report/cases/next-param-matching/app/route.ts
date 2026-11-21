export const unstable_paramMatching = { id: "dynamic" };
export function unstable_generateParamMatching() {
  return { id: "blocking" };
}

export function GET() {
  return new Response("ok");
}
