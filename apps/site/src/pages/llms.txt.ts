import type { APIRoute } from "astro";

export const GET: APIRoute = ({ site }) => {
  const base = site ?? new URL("https://getshrike.dev");

  const body = `# Shrike

> Shrike is an open-source CLI that finds the code a pull request orphaned, deletes it in verified batches, and leaves a receipt. It runs locally on your TypeScript repository and never commits until your checks pass.

## Product

- [Shrike landing page](${new URL("/", base).href}): what Shrike measures, finds, cleans, proves, and protects.
- [npm package](https://www.npmjs.com/package/getshrike): install with \`npm install -D getshrike\`.
- [Documentation](https://github.com/getshrike/shrike#documentation): command reference and guides.
- [GitHub repository](https://github.com/getshrike/shrike): source, issues, and release notes.
- [Changelog](https://github.com/getshrike/shrike/blob/main/CHANGELOG.md): release history.
- [Benchmarks](https://github.com/getshrike/shrike#performance): analysis performance and fixture results.

## Notes

- Shrike is MIT licensed and free to use.
- The report is read-only; \`clean\` applies verified batches in a worktree.
- Every cleanup batch is checked before and after, and a rejected batch is reverted.
`;

  return new Response(body, {
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
    },
  });
};
