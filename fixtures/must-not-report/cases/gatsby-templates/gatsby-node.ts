import path from "path";

export async function createPages({ actions }: { actions: any }) {
  actions.createPage({
    path: "/blog/hello",
    component: path.resolve("./src/templates/blog-post.tsx"),
    context: { slug: "hello" },
  });
}
