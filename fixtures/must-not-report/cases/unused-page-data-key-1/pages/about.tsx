interface AboutProps {
  used: number;
  unused: string;
}

function About(props: AboutProps) {
  return <div>{props.used}</div>;
}

About.getInitialProps = async () => ({ used: 1, unused: "x" });

export default About;

function unusedAboutHelper() {
  return 2;
}
