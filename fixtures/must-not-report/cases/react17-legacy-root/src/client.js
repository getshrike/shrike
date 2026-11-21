import React from "react";
import ReactDOM from "react-dom";
import App from "./App";
import { formatting } from "./format";

ReactDOM.render(<App label={formatting()} />, document.getElementById("root"));
