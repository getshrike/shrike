import { forwardRef, memo } from "react";

type BadgeProps = { label: string; tone: string; flag?: boolean };

function Badge({ label, tone, flag }: BadgeProps) {
  return <span>{label}{flag ? "!" : ""}</span>;
}

interface TypedProps {
  label: string;
  tone: string;
  flag?: boolean;
}

function Typed(props: TypedProps) {
  return <span>{props.label}{props.flag ? "!" : ""}</span>;
}

const Memoized = memo((props: TypedProps) => (
  <span>{props.label}{props.flag ? "!" : ""}</span>
));

const Forwarded = forwardRef<HTMLSpanElement, TypedProps>((props, ref) => (
  <span ref={ref}>{props.label}{props.flag ? "!" : ""}</span>
));

function Dead({ title }: { title: string }) {
  return <strong>{title}</strong>;
}

export function Widget() {
  return (
    <>
      <Badge label="hello" tone="danger" />
      <Typed label="hello" tone="danger" />
      <Memoized label="hello" tone="danger" />
      <Forwarded label="hello" tone="danger" />
    </>
  );
}
