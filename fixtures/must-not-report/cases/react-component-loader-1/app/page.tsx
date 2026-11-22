import React from "react";
import * as ReactNamespace from "react";
import { lazy as component } from "react";
import dynamic from "next/dynamic";

const LazyView = component(() => import("../src/LazyView"));
const NamespaceView = ReactNamespace.lazy(() => import("../src/NamespaceView"));
const NamedView = React.lazy(() =>
  import("../src/LazyView").then((module) => ({ default: module.NamedView })),
);
const DynamicView = dynamic(() => import("../src/DynamicView"), { ssr: false });

const loadLocalLazy = () => import("../src/LocalLazyView");
const LocalLazyView = component(loadLocalLazy);

function loadLocalDynamic() {
  return import("../src/LocalDynamicView");
}
const LocalDynamicView = dynamic(loadLocalDynamic);

const loadShared = () => import("../src/SharedLoaderView");
const SharedLoaderView = React.lazy(loadShared);
void loadShared();

export const loadExported = () => import("../src/ExportedLoaderView");
const ExportedLoaderView = React.lazy(loadExported);

let loadReassignable = () => import("../src/ReassignableLoaderView");
const ReassignableLoaderView = React.lazy(loadReassignable);
loadReassignable = () => import("../src/ReassignableLoaderView");

function loadView(loader: () => Promise<unknown>) {
  return loader();
}
const LookalikeView = loadView(() => import("../src/LookalikeView"));

function shadowed(lazy: (loader: () => Promise<unknown>) => unknown) {
  return lazy(() => import("../src/ShadowedView"));
}

export default function Page() {
  return (
    <main>
      <LazyView />
      <NamespaceView />
      <NamedView />
      <DynamicView />
      <LocalLazyView />
      <LocalDynamicView />
      <SharedLoaderView />
      <ExportedLoaderView />
      <ReassignableLoaderView />
      <LookalikeView />
      {shadowed(() => Promise.resolve({}))}
    </main>
  );
}
