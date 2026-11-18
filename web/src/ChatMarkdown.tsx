import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

const plugins = [remarkGfm];
export default function ChatMarkdown({ text }: { text: string }) {
  return <ReactMarkdown remarkPlugins={plugins}>{text}</ReactMarkdown>;
}
