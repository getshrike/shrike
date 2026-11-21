import { PostBody } from "../components/post-body";

export default function BlogPost({ pageContext }: { pageContext: { title: string } }) {
  return <PostBody title={pageContext.title} />;
}

export const query = `
  query BlogPost($slug: String!) {
    post(slug: { eq: $slug }) { title }
  }
`;

export function Head() {
  return <title>Blog post</title>;
}

export function unusedTemplateHelper() {
  return 2;
}
