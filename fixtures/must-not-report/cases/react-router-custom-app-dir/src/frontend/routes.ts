import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [
  index("routes/home.tsx"),
  route("about", "route-modules/about.tsx"),
] satisfies RouteConfig;
