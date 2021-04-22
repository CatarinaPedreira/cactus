import { Server } from "http";
import { Server as SecureServer } from "https";
import { ApiPromise, WsProvider } from "@polkadot/api";

import "multer";
import { Optional } from "typescript-optional";

import { PluginRegistry } from "@hyperledger/cactus-core";

import {
  ConsensusAlgorithmFamily,
  PluginAspect,
  IPluginWebService,
  IWebServiceEndpoint,
  ICactusPlugin,
  ICactusPluginOptions,
} from "@hyperledger/cactus-core-api";

import {
  Logger,
  Checks,
  LogLevelDesc,
  LoggerProvider,
} from "@hyperledger/cactus-common";
import { promisify } from "util";

// Should work further on this
export interface IPluginLedgerConnectorPolkadotOptions
  extends ICactusPluginOptions {
  logLevel?: LogLevelDesc;
  pluginRegistry: PluginRegistry;
  wsProviderUrl: WsProvider;
  instanceId: string;
}

// Should also implement IPluginLedgerConnector
export class PluginLedgerConnectorPolkadot
  implements ICactusPlugin, IPluginWebService {
  public static readonly CLASS_NAME = "PluginLedgerConnectorPolkadot";
  public readonly DEFAULT_WSPROVIDER = "wss://rpc.polkadot.io";
  private readonly instanceId: string;
  private readonly log: Logger;
  private wsProvider: WsProvider;

  public get className(): string {
    return PluginLedgerConnectorPolkadot.CLASS_NAME;
  }

  constructor(public readonly opts: IPluginLedgerConnectorPolkadotOptions) {
    const fnTag = `${this.className}#constructor()`;
    Checks.truthy(opts, `${fnTag} arg options`);
    if (typeof opts.logLevel !== "undefined") {
      Checks.truthy(opts.logLevel, `${fnTag} options.logLevelDesc`);
    }
    Checks.truthy(opts.pluginRegistry, `${fnTag} options.pluginRegistry`);
    Checks.truthy(opts.wsProviderUrl, `${fnTag} options.wsProviderUrl`);
    Checks.truthy(opts.instanceId, `${fnTag} options.instanceId`);

    const level = this.opts.logLevel || "INFO";
    const label = this.className;
    this.log = LoggerProvider.getOrCreate({ level, label });

    this.instanceId = opts.instanceId;
    this.wsProvider = new WsProvider(this.DEFAULT_WSPROVIDER);
  }

  public setProvider(wsProviderUrl: string): void {
    this.wsProvider = new WsProvider(wsProviderUrl);
  }

  public async createAPI(): Promise<ApiPromise> {
    const api = await ApiPromise.create({ provider: this.wsProvider });
    this.log.info("create polkadot api");
    return api;
  }

  public async installWebServices(): Promise<IWebServiceEndpoint[]> {
    throw Error("Not implemented yet");
  }

  public async shutdown(): Promise<void> {
    const serverMaybe = this.getHttpServer();
    if (serverMaybe.isPresent()) {
      const server = serverMaybe.get();
      await promisify(server.close.bind(server))();
    }
  }

  public getInstanceId(): string {
    return this.instanceId;
  }

  public getPackageName(): string {
    return `@hyperledger/cactus-plugin-ledger-connector-polkadot`;
  }

  public getAspect(): PluginAspect {
    return PluginAspect.LEDGER_CONNECTOR;
  }

  public getHttpServer(): Optional<Server | SecureServer> {
    return Optional.empty();
  }

  public async getConsensusAlgorithmFamily(): Promise<
    ConsensusAlgorithmFamily
  > {
    return ConsensusAlgorithmFamily.STAKE;
  }
}
