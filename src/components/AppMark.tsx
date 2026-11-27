import logo from "@/assets/logo.png";
import { cn } from "@/lib/utils";

export function AppMark({ className, alt = "" }: { className?: string; alt?: string }) {
  return <img src={logo} alt={alt} width={486} height={512} className={cn("block h-auto w-16", className)} />;
}
