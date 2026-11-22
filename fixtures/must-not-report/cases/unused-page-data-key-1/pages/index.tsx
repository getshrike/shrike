export async function getStaticProps() {
  return { props: { used: 1, unused: 2 } };
}

export default function Home({ used }: { used: number }) {
  return <div>{used}</div>;
}

function unusedPageHelper() {
  return 1;
}
