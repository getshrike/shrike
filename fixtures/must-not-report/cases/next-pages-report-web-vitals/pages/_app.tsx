export default function App({ Component, pageProps }: any) {
  return <Component {...pageProps} />;
}

export function reportWebVitals(metric: unknown) {
  console.log(metric);
}

export function getInitialProps() {
  return {};
}

export function unusedAppExport() {
  return 1;
}
