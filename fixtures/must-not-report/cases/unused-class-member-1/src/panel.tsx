import { Component } from "react";

export class Panel extends Component<{ label: string }> {
  state = { open: false };

  componentDidMount() {
    this.setState({ open: true });
  }

  render() {
    return (
      <section>
        {this.props.label}
        {this.state.open ? "open" : "closed"}
      </section>
    );
  }

  deadHandler() {
    return "dead";
  }
}
