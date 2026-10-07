import { useEffect, useRef } from "react";

type Find = () => void;

/** Searches on screen, latest mounted last; Cmd/Ctrl+F opens the last one. */
const finds: { current: Find }[] = [];

/** Makes `find` what Cmd/Ctrl+F opens while the calling component is mounted. */
export function useFind(find: Find) {
  const latest = useRef(find);
  useEffect(() => {
    latest.current = find;
  });
  useEffect(() => {
    finds.push(latest);
    return () => {
      finds.splice(finds.indexOf(latest), 1);
    };
  }, []);
}

/** Opens the search the view on screen offers; false when it offers none. */
export function openFind(): boolean {
  const find = finds.at(-1);
  find?.current();
  return find !== undefined;
}
