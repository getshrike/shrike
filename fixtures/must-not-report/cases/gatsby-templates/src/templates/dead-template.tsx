export default function DeadTemplate() {
  return <div>never registered</div>;
}

export const query = `query Dead { post { title } }`;

export function unusedDeadHelper() {
  return 3;
}
