import { useEffect, useRef } from "react";
import type { ButtonHTMLAttributes, FormEvent, ReactNode } from "react";
import {
  AlertCircle,
  Check,
  ChevronDown,
  Copy,
  LoaderCircle,
  Search,
  X,
} from "lucide-react";
import { copy, errorMessage } from "./api";
import { Mascot } from "./Mascot";

export type Notice = (text: string, error?: boolean) => void;

export function Button({
  children,
  className = "",
  busy,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { busy?: boolean }) {
  return (
    <button
      className={`button ${className}`}
      {...props}
      disabled={props.disabled || busy}
    >
      {busy ? <LoaderCircle className="spin" size={16} /> : null}
      {children}
    </button>
  );
}

export function IconButton({
  title,
  children,
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { title: string }) {
  return (
    <button
      type="button"
      className={`icon-button ${className}`}
      title={title}
      aria-label={title}
      {...props}
    >
      {children}
    </button>
  );
}

export function CopyButton({
  value,
  notify,
  title = "复制",
}: {
  value: string;
  notify: Notice;
  title?: string;
}) {
  return (
    <IconButton
      title={title}
      onClick={() => {
        void copy(value)
          .then(() => notify("已复制到剪贴板"))
          .catch((error) => notify(errorMessage(error), true));
      }}
    >
      <Copy size={16} />
    </IconButton>
  );
}

export function Badge({
  children,
  variant = "neutral",
  dot = false,
}: {
  children: ReactNode;
  variant?: "green" | "neutral" | "amber" | "red";
  dot?: boolean;
}) {
  return (
    <span className={`badge ${variant}`}>
      {dot && <i />}
      {children}
    </span>
  );
}

export function ErrorBox({ message }: { message: string }) {
  return message ? (
    <div className="error-box" role="alert">
      <AlertCircle size={17} />
      <span>{message}</span>
    </div>
  ) : null;
}

export function Empty({
  title,
  detail,
  action,
}: {
  title: string;
  detail?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">
        <Mascot size={58} />
      </div>
      <strong>{title}</strong>
      {detail && <p>{detail}</p>}
      {action}
    </div>
  );
}

export function SearchInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  return (
    <label className="search">
      <Search size={16} />
      <input
        aria-label={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
      />
      {value && (
        <button
          type="button"
          aria-label="清空搜索"
          onClick={() => onChange("")}
        >
          <X size={14} />
        </button>
      )}
    </label>
  );
}

export function Select({
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="select-wrap">
      <select {...props}>{children}</select>
      <ChevronDown size={15} />
    </span>
  );
}

export function Field({
  label,
  children,
  hint,
  optional = false,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
  optional?: boolean;
}) {
  return (
    <label className="field">
      <span className="field-label">
        {label}
        {optional && <span>可选</span>}
      </span>
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  describedBy,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  describedBy?: string;
}) {
  return (
    <label className="toggle-label">
      <input
        type="checkbox"
        role="switch"
        aria-describedby={describedBy}
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="toggle-track">
        <span />
      </span>
      <span>{label}</span>
    </label>
  );
}

export function Modal({
  title,
  subtitle,
  children,
  onClose,
  wide = false,
  busy = false,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);
  return (
    <dialog
      ref={ref}
      className={`modal ${wide ? "wide" : ""}`}
      onCancel={(event) => {
        event.preventDefault();
      }}
      aria-labelledby="modal-title"
    >
      <div className="modal-header">
        <div>
          <h2 id="modal-title">{title}</h2>
          {subtitle && <p>{subtitle}</p>}
        </div>
        <IconButton title="取消" onClick={onClose} disabled={busy}>
          <X size={19} />
        </IconButton>
      </div>
      {children}
    </dialog>
  );
}

export function ModalForm({
  children,
  onSubmit,
  onClose,
  busy,
  submit = "保存",
  error,
}: {
  children: ReactNode;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onClose: () => void;
  busy: boolean;
  submit?: string;
  error: string;
}) {
  return (
    <form onSubmit={onSubmit}>
      <div className="modal-body">
        {children}
        <ErrorBox message={error} />
      </div>
      <div className="modal-footer">
        <Button type="button" onClick={onClose} disabled={busy}>
          取消
        </Button>
        <Button type="submit" className="primary" busy={busy}>
          {submit}
        </Button>
      </div>
    </form>
  );
}

export function Confirm({
  title,
  message,
  onClose,
  onConfirm,
  busy,
  error,
}: {
  title: string;
  message: string;
  onClose: () => void;
  onConfirm: () => void;
  busy: boolean;
  error: string;
}) {
  return (
    <Modal title={title} onClose={onClose} busy={busy}>
      <div className="modal-body">
        <p className="confirm-message">{message}</p>
        <ErrorBox message={error} />
      </div>
      <div className="modal-footer">
        <Button onClick={onClose} disabled={busy}>
          取消
        </Button>
        <Button className="danger" onClick={onConfirm} busy={busy}>
          确认删除
        </Button>
      </div>
    </Modal>
  );
}

export function Toast({
  text,
  error,
  onClose,
}: {
  text: string;
  error?: boolean;
  onClose: () => void;
}) {
  return (
    <div
      className={`toast ${error ? "toast-error" : ""}`}
      role={error ? "alert" : "status"}
    >
      {error ? <AlertCircle size={17} /> : <Check size={17} />}
      <span>{text}</span>
      <IconButton title="关闭通知" onClick={onClose}>
        <X size={14} />
      </IconButton>
    </div>
  );
}
